use acu_assets::{Limits, frame, package, reader::Reader};

fn raw_frame(data: &[u8]) -> Vec<u8> {
    let mut result = frame::MAGIC.to_le_bytes().to_vec();
    result.extend_from_slice(&1u16.to_le_bytes());
    result.push(1);
    result.extend_from_slice(&0x80008000u32.to_le_bytes());
    result.extend_from_slice(&1u32.to_le_bytes());
    result.extend_from_slice(&(data.len() as u16).to_le_bytes());
    result.extend_from_slice(&(data.len() as u16).to_le_bytes());
    // Adler-32 seeded with zero; independent straightforward fixture calculation.
    let (mut a, mut b) = (0u32, 0u32);
    for byte in data {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    result.extend_from_slice(&(b << 16 | a).to_le_bytes());
    result.extend_from_slice(data);
    result
}

fn many_literal_blocks(indexed: bool) -> Vec<u8> {
    let mut b = frame::MAGIC.to_le_bytes().to_vec();
    b.extend_from_slice(&1u16.to_le_bytes());
    b.push(1);
    b.extend_from_slice(&(if indexed { 0x80008000u32 } else { 0 }).to_le_bytes());
    if indexed {
        b.extend_from_slice(&65u32.to_le_bytes());
        for _ in 0..65 {
            b.extend_from_slice(&[3, 0, 7, 0]);
        }
    }
    for _ in 0..65 {
        if !indexed {
            b.push(1);
            b.extend_from_slice(&7u32.to_le_bytes());
            b.extend_from_slice(&3u32.to_le_bytes());
        }
        b.extend_from_slice(&[235, 0, 155, 4, 20, b'A', b'B', b'C', 17, 0, 0]);
    }
    b
}

struct YieldOnce(bool);
impl std::future::Future for YieldOnce {
    type Output = ();
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.0 {
            std::task::Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    }
}

#[test]
fn cooperative_decode_really_suspends_in_both_block_loops() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    for indexed in [false, true] {
        let bytes = many_literal_blocks(indexed);
        let mut r = Reader::new(&bytes);
        let mut checkpoint = || YieldOnce(false);
        let mut pending = 0;
        let (decoded, info) = {
            let mut task = std::pin::pin!(frame::decode_cooperative(
                &mut r,
                Limits::default(),
                &mut checkpoint
            ));
            loop {
                match task.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                    Poll::Pending => {
                        pending += 1;
                        assert!(pending <= 2);
                    }
                    Poll::Ready(result) => break result.unwrap(),
                }
            }
        };
        assert_eq!(pending, 2, "must suspend after blocks 32 and 64");
        assert_eq!(decoded, b"ABC".repeat(65));
        assert_eq!(info.blocks, 65);
        assert_eq!(r.position, bytes.len());
        assert_eq!(
            frame::decode(&mut Reader::new(&bytes), Limits::default())
                .unwrap()
                .0,
            decoded
        );
        // Corruption after the first yield must remain an error, not a partial success.
        let mut corrupt = bytes.clone();
        let end = corrupt.len();
        corrupt[end - 11] ^= 1;
        let mut r = Reader::new(&corrupt);
        let mut task = std::pin::pin!(frame::decode_cooperative(
            &mut r,
            Limits::default(),
            &mut checkpoint
        ));
        loop {
            if let Poll::Ready(result) = task.as_mut().poll(&mut Context::from_waker(Waker::noop()))
            {
                assert!(result.is_err());
                break;
            }
        }
    }
}

#[test]
fn cooperative_decode_can_be_cancelled_at_a_block_boundary() {
    use std::{
        future::Future,
        task::{Context, Waker},
    };
    let bytes = many_literal_blocks(true);
    let mut r = Reader::new(&bytes);
    let mut checkpoint = || YieldOnce(false);
    let mut task = Box::pin(frame::decode_cooperative(
        &mut r,
        Limits::default(),
        &mut checkpoint,
    ));
    assert!(
        task.as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(task);
    assert_eq!(r.position, 19 + 65 * 4 + 32 * 11);
    assert!(r.position < bytes.len());
}

#[test]
fn independent_lzo_literal_and_checksum() {
    // A handwritten LZO literal stream for ABC, not produced by our decoder.
    let encoded = [20, b'A', b'B', b'C', 17, 0, 0];
    let mut b = frame::MAGIC.to_le_bytes().to_vec();
    b.extend_from_slice(&[1, 0, 1, 0, 128, 0, 128, 1, 0, 0, 0, 3, 0, 7, 0]);
    // Checked independently with Python zlib.adler32(encoded,0).
    b.extend_from_slice(&[235, 0, 155, 4]);
    b.extend_from_slice(&encoded);
    let (decoded, info) = frame::decode(&mut Reader::new(&b), Limits::default()).unwrap();
    assert_eq!(decoded, b"ABC");
    assert!(info.checksum_matches_compressed);
}

#[test]
fn truncated_corrupt_and_over_limit_frames_fail() {
    let b = raw_frame(b"ABC");
    for end in 0..b.len() {
        assert!(
            frame::decode(&mut Reader::new(&b[..end]), Limits::default()).is_err(),
            "prefix {end}"
        );
    }
    let mut corrupt = b.clone();
    corrupt[27] ^= 1;
    assert!(frame::decode(&mut Reader::new(&corrupt), Limits::default()).is_err());
    let limits = Limits {
        max_decoded: 2,
        ..Limits::default()
    };
    assert!(frame::decode(&mut Reader::new(&b), limits).is_err());
    let mut count = b;
    count[15..19].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(frame::decode(&mut Reader::new(&count), Limits::default()).is_err());
}

#[test]
fn metadata_identity_record_bounds_and_opaque_status() {
    let mut metadata = 1u16.to_le_bytes().to_vec();
    metadata.extend_from_slice(&0xfedcba9876543210u64.to_le_bytes());
    metadata.extend_from_slice(&17u32.to_le_bytes());
    metadata.extend_from_slice(&0u16.to_le_bytes());
    let mut record = 0x12345678u32.to_le_bytes().to_vec();
    record.extend_from_slice(&3u32.to_le_bytes());
    record.extend_from_slice(&1u32.to_le_bytes());
    record.extend_from_slice(b"X\0ABC");
    let mut raw = raw_frame(&metadata);
    raw.extend_from_slice(&raw_frame(&record));
    let p = package::decode(&raw, Limits::default()).unwrap();
    let r = &p.resources[0];
    assert_eq!(
        (r.file_id.as_str(), r.type_id.as_str(), r.name.as_str()),
        ("fedcba9876543210", "12345678", "X")
    );
    assert_eq!(&p.bytes[r.payload.clone()], b"ABC");
    assert_eq!(r.typed_status, "unsupported");
    let mut tagged = metadata.clone();
    tagged[14..16].copy_from_slice(&1u16.to_le_bytes());
    tagged.extend_from_slice(&7u16.to_le_bytes());
    let mut tagged_raw = raw_frame(&tagged);
    tagged_raw.extend_from_slice(&raw_frame(&record));
    let parsed = package::decode(&tagged_raw, Limits::default()).unwrap();
    assert_eq!(parsed.resources[0].metadata_tags, vec![7]);
    assert!(parsed.metadata_extension.is_empty());
    raw.push(0);
    assert!(package::decode(&raw, Limits::default()).is_err());
}

#[test]
fn default_mode_stops_before_next_frame_and_checks_size() {
    let mut b = frame::MAGIC.to_le_bytes().to_vec();
    b.extend_from_slice(&[1, 0, 0, 0, 128, 0, 0]);
    b.push(1);
    b.extend_from_slice(&7u32.to_le_bytes());
    b.extend_from_slice(&3u32.to_le_bytes());
    b.extend_from_slice(&0x049b00ebu32.to_le_bytes());
    b.extend_from_slice(&[20, b'A', b'B', b'C', 17, 0, 0]);
    b.extend_from_slice(&raw_frame(b"XY"));
    let mut r = Reader::new(&b);
    let (first, _) = frame::decode(&mut r, Limits::default()).unwrap();
    assert_eq!(first, b"ABC");
    assert_eq!(r.position, 35);
    let (second, _) = frame::decode(&mut r, Limits::default()).unwrap();
    assert_eq!(second, b"XY");
    assert_eq!(r.remaining(), 0);
    b[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(frame::decode(&mut Reader::new(&b), Limits::default()).is_err());
}

#[test]
fn wide_length_toc_variants_are_bounded() {
    for version in [0u16, 2, 3] {
        let mut b = frame::MAGIC.to_le_bytes().to_vec();
        b.extend_from_slice(&version.to_le_bytes());
        b.push(1);
        b.extend_from_slice(&0x80008000u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&0x018a00c6u32.to_le_bytes());
        b.extend_from_slice(b"ABC");
        assert_eq!(
            frame::decode(&mut Reader::new(&b), Limits::default())
                .unwrap()
                .0,
            b"ABC"
        );
        b[19..23].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(frame::decode(&mut Reader::new(&b), Limits::default()).is_err());
    }
}
