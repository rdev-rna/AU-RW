//! Borrowed native texture data. ACU-RE-0003 and ACU-RE-0005 texture variants.
use crate::{Result, invalid, reader::Reader};
use serde::Serialize;
use std::ops::Range;

pub const TYPE_TEXTURE_MAP: &str = "a2b7e917";

/// A decoded resource is not necessarily an uploadable texture. The observed
/// dynamic font placeholder has a complete header and no compiled pixel bytes.
#[derive(Debug, Serialize)]
#[serde(tag = "storage")]
pub enum TextureResource<'a> {
    Resident(Texture2D<'a>),
    NoInlinePixels(TextureDescriptor),
}

#[derive(Debug, Serialize)]
pub struct TextureDescriptor {
    pub file_id: String,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub engine_format: u32,
    pub kind: u32,
    pub gamma: u32,
    pub mip_count: u32,
    pub map_type: u32,
    pub category: u32,
    pub dynamic: bool,
    pub ignore_skip_mips: bool,
    pub user_category: u32,
    pub top_mip_tag: u8,
    pub top_mip_id: Option<String>,
    pub compiled_marker: u8,
}

#[derive(Debug, Serialize)]
pub struct Texture2D<'a> {
    pub file_id: String,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    /// Native kind: 0=1D, 1=2D, 2=cube, 3=volume.
    pub kind: u32,
    pub layers: u32,
    pub compiled_width: u32,
    pub compiled_height: u32,
    pub mip_count: u32,
    pub engine_format: u32,
    pub dxgi_format: u32,
    pub gamma: u32,
    pub map_type: u32,
    pub category: u32,
    pub dynamic: bool,
    pub ignore_skip_mips: bool,
    pub user_category: u32,
    pub top_mip_tag: u8,
    pub compiled_id: String,
    pub platform_version: u32,
    pub sdk_version: u32,
    pub top_mip_size: u32,
    pub total_texture_size: u32,
    pub alignment: u32,
    /// Face-major for cubes; each volume mip includes all of its depth slices.
    /// Cube face orientation remains unverified.
    pub mips: Vec<Range<usize>>,
    #[serde(skip)]
    pub pixels: &'a [u8],
}

fn format_info(index: u32) -> Result<(u32, u32, u32)> {
    // (DXGI format, bytes per element, element side). ACU-RE-0003 mapping.
    Ok(match index {
        0 => (87, 4, 1),
        1 => (31, 4, 1),
        2 | 3 => (71, 8, 4),
        4 => (74, 16, 4),
        5 | 6 => (77, 16, 4),
        7 => (95, 16, 4),
        8 => (98, 16, 4),
        9 => (65, 1, 1),
        10 => (61, 1, 1),
        11 => (56, 2, 1),
        12 => (49, 2, 1),
        13 => (41, 4, 1),
        14 => (2, 16, 1),
        15 => (10, 8, 1),
        16 => (35, 4, 1),
        17 => (34, 4, 1),
        _ => return invalid(format!("unsupported Unity pixel format {index}")),
    })
}

fn boolean(r: &mut Reader<'_>) -> Result<bool> {
    match r.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => invalid("invalid texture boolean"),
    }
}

fn descriptor(r: &mut Reader<'_>, expected_id: &str) -> Result<TextureDescriptor> {
    if r.u8()? != 1 {
        return invalid("unsupported texture root wrapper");
    }
    let id = r.u64()?;
    if format!("{id:016x}") != expected_id || r.u32()? != 0xa2b7e917 {
        return invalid("texture resource identity mismatch");
    }
    let width = r.u32()?;
    let height = r.u32()?;
    let depth = r.u32()?;
    let engine_format = r.u32()?;
    let kind = r.u32()?;
    let gamma = r.u32()?;
    let mip_count = r.u32()?.max(1);
    let map_type = r.u32()?;
    let category = r.u32()?;
    if width == 0
        || height == 0
        || width > 65536
        || height > 65536
        || depth == 0
        || depth > 65536
        || !matches!(kind, 0..=3)
        || (kind != 3 && depth != 1)
        || (kind == 0 && height != 1)
        || (kind == 2 && width != height)
        || mip_count > 32
    {
        return invalid("unsupported texture dimensions/kind/mips");
    }
    format_info(engine_format)?;
    let dynamic = boolean(r)?;
    let ignore_skip_mips = boolean(r)?;
    let user_category = r.u32()?;
    let top_mip_tag = r.u8()?;
    let top_mip = r.u64()?;
    Ok(TextureDescriptor {
        file_id: expected_id.into(),
        width,
        height,
        depth,
        engine_format,
        kind,
        gamma,
        mip_count,
        map_type,
        category,
        dynamic,
        ignore_skip_mips,
        user_category,
        top_mip_tag,
        top_mip_id: (top_mip != 0).then(|| format!("{top_mip:016x}")),
        compiled_marker: r.u8()?,
    })
}

/// Fully decode the supported inline variants, including an observed dynamic
/// placeholder. `NoInlinePixels` supplies no pixel storage for renderer upload.
pub fn parse_resource<'a>(payload: &'a [u8], expected_id: &str) -> Result<TextureResource<'a>> {
    let mut r = Reader::new(payload);
    let header = descriptor(&mut r, expected_id)?;
    if header.compiled_marker == 0 {
        return Ok(TextureResource::Resident(parse(payload, expected_id)?));
    }
    if header.compiled_marker != 3 || !header.dynamic || header.top_mip_id.is_some() {
        return invalid("unsupported texture without inline compiled pixels");
    }
    if r.remaining() != 0 {
        return invalid("trailing texture placeholder bytes");
    }
    Ok(TextureResource::NoInlinePixels(header))
}

/// Decode pixels usable by a CPU/GPU consumer. Pixel-less resources and
/// unresolved external top mips always fail this API.
pub fn parse<'a>(payload: &'a [u8], expected_id: &str) -> Result<Texture2D<'a>> {
    let mut r = Reader::new(payload);
    let TextureDescriptor {
        width,
        height,
        depth,
        engine_format,
        kind,
        gamma,
        mip_count,
        map_type,
        category,
        dynamic,
        ignore_skip_mips,
        user_category,
        top_mip_tag,
        top_mip_id,
        compiled_marker,
        ..
    } = descriptor(&mut r, expected_id)?;
    if let Some(id) = top_mip_id {
        return invalid(format!("unresolved external texture mip {id}"));
    }
    if compiled_marker != 0 {
        return invalid("texture contains no inline compiled pixel data");
    }
    let compiled_id = r.u64()?;
    if r.u32()? != 0x13237fe9 {
        return invalid("unexpected compiled texture class");
    }
    let platform_version = r.u32()?;
    let sdk_version = r.u32()?;
    let compiled_width = r.u32()?;
    let compiled_height = r.u32()?;
    let compiled_depth = r.u32()?;
    let (dxgi_format, element_bytes, side) = format_info(engine_format)?;
    // BC resources can pad a root dimension smaller than one 4x4 block.
    // Do not accept arbitrary descriptor disagreements.
    let dimension_matches = |root: u32, compiled: u32| {
        compiled == root || (side == 4 && root < side && compiled == side)
    };
    if !dimension_matches(width, compiled_width)
        || !dimension_matches(height, compiled_height)
        || compiled_depth != depth
    {
        return invalid("root/compiled texture dimensions disagree");
    }
    if r.u32()?.max(1) != mip_count
        || r.u32()? != engine_format
        || r.u32()? != kind
        || r.u32()? != gamma
    {
        return invalid("root/compiled texture descriptors disagree");
    }
    let top_mip_size = r.u32()?;
    let total_texture_size = r.u32()?;
    let alignment = r.u32()?;
    let length = r.u32()? as usize;
    let pixels = r.take(length)?;
    if r.remaining() != 0 {
        return invalid("trailing texture payload bytes");
    }
    let layers = if kind == 2 { 6 } else { 1 };
    let mut mips = Vec::new();
    let mut cursor = 0usize;
    for _ in 0..layers {
        let (mut w, mut h, mut d) = (width, height, depth);
        for _ in 0..mip_count {
            let size = u64::from(w.div_ceil(side))
                * u64::from(h.div_ceil(side))
                * u64::from(element_bytes)
                * u64::from(d);
            let end = cursor
                .checked_add(usize::try_from(size)?)
                .ok_or("texture mip size overflow")?;
            if end > pixels.len() {
                return invalid("texture mip outside pixel block");
            }
            mips.push(cursor..end);
            cursor = end;
            w = (w / 2).max(1);
            h = (h / 2).max(1);
            d = (d / 2).max(1);
        }
    }
    if cursor != pixels.len() {
        return invalid("pixel byte count does not match mip descriptors");
    }
    Ok(Texture2D {
        file_id: expected_id.into(),
        width,
        height,
        depth,
        kind,
        layers,
        compiled_width,
        compiled_height,
        mip_count,
        engine_format,
        dxgi_format,
        gamma,
        map_type,
        category,
        dynamic,
        ignore_skip_mips,
        user_category,
        top_mip_tag,
        compiled_id: format!("{compiled_id:016x}"),
        platform_version,
        sdk_version,
        top_mip_size,
        total_texture_size,
        alignment,
        mips,
        pixels,
    })
}
