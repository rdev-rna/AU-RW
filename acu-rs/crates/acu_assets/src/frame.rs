//! Investigational ACU framing. ACU-RE-0002; unsupported variants fail explicitly.
use crate::{Limits, Result, invalid, reader::Reader};
use serde::Serialize;

pub const MAGIC: u64 = 0x1004fa9957fbaa33;

#[derive(Debug, Serialize)]
pub struct FrameInfo {
    pub version: u16,
    pub algorithm: u8,
    pub flags: u32,
    pub blocks: usize,
    pub decoded_length: usize,
    pub stored_length: usize,
    pub checksum_matches_compressed: bool,
    pub checksum_matches_decoded: bool,
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (0u32, 0u32);
    for chunk in data.chunks(5552) {
        for byte in chunk {
            a += u32::from(*byte);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    b << 16 | a
}

pub fn decode(r: &mut Reader<'_>, limits: Limits) -> Result<(Vec<u8>, FrameInfo)> {
    // The synchronous API uses always-ready checkpoints, so one poll completes.
    use std::{
        future::{Future, ready},
        task::{Context, Poll, Waker},
    };
    let mut checkpoint = || ready(());
    let mut future = std::pin::pin!(decode_cooperative(r, limits, &mut checkpoint));
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => unreachable!("synchronous decoder checkpoint cannot suspend"),
    }
}

/// Same bounded decoder with executor-owned checkpoints every 32 blocks.
/// A single LZO block is indivisible; cancellation is safe between blocks.
/// No executor or Bevy dependency is required by the asset reader.
pub async fn decode_cooperative<F, Fut>(
    r: &mut Reader<'_>,
    limits: Limits,
    checkpoint: &mut F,
) -> Result<(Vec<u8>, FrameInfo)>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let start = r.position;
    if r.u64()? != MAGIC {
        return invalid("unsupported compressed frame magic");
    }
    let version = r.u16()?;
    let algorithm = r.u8()?;
    let flags = r.u32()?;
    if !matches!(version, 0..=3) || !matches!(algorithm, 0 | 1) {
        return invalid(format!(
            "unsupported frame version={version}, algorithm={algorithm}, flags={flags:08x}"
        ));
    }
    if flags & 0x80000000 == 0 {
        let mut output = Vec::new();
        let mut blocks = 0;
        while r.remaining() > 0 && matches!(r.bytes[r.position], 0 | 1) {
            if blocks >= limits.max_resources {
                return invalid("default frame block limit exceeded");
            }
            let compressed = r.u8()? == 1;
            let stored = r.u32()? as usize;
            let decoded = if compressed {
                r.u32()? as usize
            } else {
                stored
            };
            let checksum = if compressed { Some(r.u32()?) } else { None };
            let total = output
                .len()
                .checked_add(decoded)
                .ok_or("frame size overflow")?;
            if total > limits.max_decoded {
                return invalid("default frame allocation limit exceeded");
            }
            let src = r.take(stored)?;
            if let Some(checksum) = checksum
                && adler32(src) != checksum
            {
                return invalid("default frame checksum mismatch");
            }
            let position = output.len();
            output.resize(total, 0);
            let dst = &mut output[position..];
            if decoded == stored {
                dst.copy_from_slice(src);
            } else {
                let written = lzokay::decompress::decompress(src, dst)
                    .map_err(|e| format!("default LZO failure: {e:?}"))?;
                if written != decoded {
                    return invalid("default LZO output size mismatch");
                }
            }
            blocks += 1;
            if blocks % 32 == 0 {
                checkpoint().await;
            }
        }
        if blocks == 0 {
            return invalid("empty/unsupported default frame");
        }
        let decoded_length = output.len();
        return Ok((
            output,
            FrameInfo {
                version,
                algorithm,
                flags,
                blocks,
                decoded_length,
                stored_length: r.position - start,
                checksum_matches_compressed: true,
                checksum_matches_decoded: false,
            },
        ));
    }
    let blocks = r.u32()? as usize;
    let width = if version == 1 { 4 } else { 8 };
    if blocks > limits.max_resources || blocks > r.remaining() / width {
        return invalid("invalid frame block count");
    }
    let mut lengths = Vec::with_capacity(blocks);
    let mut total = 0usize;
    for _ in 0..blocks {
        let decoded = if version == 1 {
            r.u16()? as usize
        } else {
            r.u32()? as usize
        };
        let stored = if version == 1 {
            r.u16()? as usize
        } else {
            r.u32()? as usize
        };
        total = total
            .checked_add(decoded)
            .ok_or("decoded length overflow")?;
        if total > limits.max_decoded {
            return invalid("decoded allocation limit exceeded");
        }
        lengths.push((decoded, stored));
    }
    let mut output = vec![0u8; total];
    let mut position = 0;
    let (mut checks_compressed, mut checks_decoded) = (true, true);
    for (block, (decoded, stored)) in lengths.into_iter().enumerate() {
        let checksum = r.u32()?;
        let src = r.take(stored)?;
        if adler32(src) != checksum {
            return invalid("frame compressed-block checksum mismatch");
        }
        let dst = &mut output[position..position + decoded];
        if decoded == stored {
            dst.copy_from_slice(src);
        } else {
            let written = lzokay::decompress::decompress(src, dst)
                .map_err(|e| format!("LZO candidate failure: {e:?}"))?;
            if written != decoded {
                return invalid("LZO output size mismatch");
            }
        }
        checks_compressed &= adler32(src) == checksum;
        checks_decoded &= adler32(dst) == checksum;
        position += decoded;
        if (block + 1) % 32 == 0 {
            checkpoint().await;
        }
    }
    Ok((
        output,
        FrameInfo {
            version,
            algorithm,
            flags,
            blocks,
            decoded_length: total,
            stored_length: r.position - start,
            checksum_matches_compressed: checks_compressed,
            checksum_matches_decoded: checks_decoded,
        },
    ))
}
