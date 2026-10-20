//! ACU resource packaging, distinct from typed asset decoding. ACU-RE-0002.
use crate::{
    Limits, Result,
    frame::{self, FrameInfo},
    invalid,
    reader::Reader,
};
use serde::Serialize;
use std::ops::Range;

#[derive(Debug, Serialize)]
pub struct Resource {
    pub file_id: String,
    pub type_id: String,
    pub name: String,
    pub record_offset: usize,
    pub record_length: usize,
    pub payload: Range<usize>,
    pub metadata_tags: Vec<u16>,
    pub typed_status: &'static str,
}

pub struct Package {
    pub bytes: Vec<u8>,
    pub resources: Vec<Resource>,
    pub frames: Vec<FrameInfo>,
    /// Preserved, unparsed metadata extension. Nonempty means incomplete metadata support.
    pub metadata_extension: Vec<u8>,
}

pub struct DecodedFrames {
    pub metadata: Vec<u8>,
    pub bytes: Vec<u8>,
    pub frames: Vec<FrameInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetadataResource {
    pub file_id: String,
    pub record_offset: usize,
    pub record_length: usize,
    pub metadata_tags: Vec<u16>,
}

#[derive(Debug, Serialize)]
pub struct PackageMetadata {
    pub records: Vec<MetadataResource>,
    pub metadata_extension: Vec<u8>,
    pub decoded_resource_length: usize,
}

/// Decode the resource table without touching the resource payload frame.
pub fn parse_metadata(metadata: &[u8], limits: Limits) -> Result<PackageMetadata> {
    if metadata.len() > limits.max_decoded {
        return invalid("resource metadata byte limit exceeded");
    }
    let mut m = Reader::new(metadata);
    let count = m.u16()? as usize;
    if count > limits.max_resources || count > m.remaining() / 14 {
        return invalid("unsupported resource metadata layout/count");
    }
    let mut records = Vec::with_capacity(count);
    let mut offset = 0usize;
    for _ in 0..count {
        let file_id = format!("{:016x}", m.u64()?);
        let record_length = m.u32()? as usize;
        let tags = m.u16()? as usize;
        if tags > m.remaining() / 2 {
            return invalid("truncated resource metadata tags");
        }
        let mut metadata_tags = Vec::with_capacity(tags);
        for _ in 0..tags {
            metadata_tags.push(m.u16()?);
        }
        records.push(MetadataResource {
            file_id,
            record_offset: offset,
            record_length,
            metadata_tags,
        });
        offset = offset
            .checked_add(record_length)
            .ok_or("resource table length overflow")?;
        if offset > limits.max_decoded {
            return invalid("resource table declares over-limit decoded stream");
        }
    }
    Ok(PackageMetadata {
        records,
        metadata_extension: m.take(m.remaining())?.to_vec(),
        decoded_resource_length: offset,
    })
}

pub fn decode_frames(raw: &[u8], limits: Limits) -> Result<DecodedFrames> {
    let mut r = Reader::new(raw);
    let (metadata, first) = frame::decode(&mut r, limits)?;
    let remaining_limits = Limits {
        max_decoded: limits.max_decoded.saturating_sub(metadata.len()),
        ..limits
    };
    let (bytes, second) = frame::decode(&mut r, remaining_limits)?;
    if r.remaining() != 0 {
        return invalid(format!("package has {} trailing bytes", r.remaining()));
    }
    Ok(DecodedFrames {
        metadata,
        bytes,
        frames: vec![first, second],
    })
}

pub fn decode(raw: &[u8], limits: Limits) -> Result<Package> {
    parse(decode_frames(raw, limits)?, limits)
}

/// Async package framing with caller-provided cooperative scheduling.
pub async fn decode_cooperative<F, Fut>(
    raw: &[u8],
    limits: Limits,
    mut checkpoint: F,
) -> Result<Package>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut r = Reader::new(raw);
    let (metadata, first) = frame::decode_cooperative(&mut r, limits, &mut checkpoint).await?;
    let remaining_limits = Limits {
        max_decoded: limits.max_decoded.saturating_sub(metadata.len()),
        ..limits
    };
    let (bytes, second) =
        frame::decode_cooperative(&mut r, remaining_limits, &mut checkpoint).await?;
    if r.remaining() != 0 {
        return invalid(format!("package has {} trailing bytes", r.remaining()));
    }
    parse(
        DecodedFrames {
            metadata,
            bytes,
            frames: vec![first, second],
        },
        limits,
    )
}

pub fn parse(decoded: DecodedFrames, limits: Limits) -> Result<Package> {
    let DecodedFrames {
        metadata,
        bytes,
        frames,
    } = decoded;
    let table = parse_metadata(&metadata, limits)?;
    if table.decoded_resource_length != bytes.len() {
        return invalid("metadata does not cover decoded package");
    }
    let mut resources = Vec::with_capacity(table.records.len());
    let mut data = Reader::new(&bytes);
    for entry in table.records {
        let length = entry.record_length;
        let offset = data.position;
        let record = data.take(length)?;
        let mut rec = Reader::new(record);
        let type_id = rec.u32()?;
        let payload_length = rec.u32()? as usize;
        let name_length = rec.u32()? as usize;
        if name_length > 4096 {
            return invalid("resource name limit exceeded");
        }
        let name = String::from_utf8(rec.take(name_length)?.to_vec())?;
        match rec.u8()? {
            0 => {}
            1 => {
                rec.take(3)?;
                let count = rec.u32()? as usize;
                if count > limits.max_resources {
                    return invalid("resource header reference limit exceeded");
                }
                rec.take(
                    count
                        .checked_mul(12)
                        .ok_or("resource header size overflow")?,
                )?;
            }
            _ => return invalid("unsupported resource header flag"),
        }
        let begin = offset + rec.position;
        rec.take(payload_length)?;
        if rec.remaining() != 0 {
            return invalid("resource record size mismatch");
        }
        resources.push(Resource {
            file_id: entry.file_id,
            type_id: format!("{type_id:08x}"),
            name,
            record_offset: offset,
            record_length: length,
            payload: begin..begin + payload_length,
            metadata_tags: entry.metadata_tags,
            typed_status: "unsupported",
        });
    }
    if data.remaining() != 0 {
        return invalid("metadata does not cover decoded package");
    }
    Ok(Package {
        bytes,
        resources,
        frames,
        metadata_extension: table.metadata_extension,
    })
}

impl Package {
    pub fn texture_resource(&self, index: usize) -> Result<crate::texture::TextureResource<'_>> {
        let r = self
            .resources
            .get(index)
            .ok_or("resource index outside package")?;
        if r.type_id != crate::texture::TYPE_TEXTURE_MAP {
            return invalid("resource is not a TextureMap");
        }
        crate::texture::parse_resource(&self.bytes[r.payload.clone()], &r.file_id)
    }

    pub fn texture(&self, index: usize) -> Result<crate::texture::Texture2D<'_>> {
        let r = self
            .resources
            .get(index)
            .ok_or("resource index outside package")?;
        if r.type_id != crate::texture::TYPE_TEXTURE_MAP {
            return invalid("resource is not a TextureMap");
        }
        crate::texture::parse(&self.bytes[r.payload.clone()], &r.file_id)
    }
}
