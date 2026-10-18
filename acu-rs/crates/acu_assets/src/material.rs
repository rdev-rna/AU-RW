//! ACU material, texture-set and dynamic parameter chain (ACU-RE-0013).
//!
//! Layout reference: AnvilToolkit 1.3.7.0 `Materials.Material`, `Materials.Mask`,
//! `Materials.Mask16`, `Materials.TextureSet`, `Common.TextureSelector`,
//! `Common.UVTransform`, `Common.FileReference` and the `Common.BaseTypes.*`
//! parameter types, read through their `Game.Unity` branches. Field names follow
//! that source; recovered ACU semantics that the source does not name stay as
//! numbers or hashes.
//!
//! Class and type hashes are the zlib CRC-32 of the class name (`Material`
//! `85c817c3`, `TextureSet` `d70e6670`, `Mask` `df5d6c0e`, `Mask16` `92b95f74`,
//! `TextureSelector` `7d08460d`, `UVTransform` `c52e2125`).
use crate::{Limits, Result, invalid, reader::Reader};
use serde::Serialize;

pub const TYPE_MATERIAL: &str = "85c817c3";
pub const TYPE_TEXTURE_SET: &str = "d70e6670";

pub const CLASS_MATERIAL: u32 = 0x85c8_17c3;
pub const CLASS_TEXTURE_SET: u32 = 0xd70e_6670;
pub const CLASS_MASK: u32 = 0xdf5d_6c0e;
pub const CLASS_MASK16: u32 = 0x92b9_5f74;
pub const CLASS_TEXTURE_SELECTOR: u32 = 0x7d08_460d;
pub const CLASS_UV_TRANSFORM: u32 = 0xc52e_2125;

/// `AnvilToolkit.Utils.TextureMapType` order, as declared by the reference tool.
/// ACU payloads carry the same numbering for the observed selectors
/// (`0` diffuse, `1` normal), but the numbering itself is not native evidence.
pub fn texture_map_type_name(value: u32) -> Option<&'static str> {
    Some(match value {
        0 => "MapDiffuse",
        1 => "MapNormal",
        2 => "MapSpecular",
        3 => "MapSpecularPower",
        4 => "MapOffsetBump",
        5 => "MapEmissive",
        6 => "MapTransmission",
        7 => "MapOcclusion",
        8 => "MapMask1",
        9 => "MapMask2",
        10 => "MapGeneric",
        11 => "RVBmask",
        12 => "MapMaxNbTypes",
        _ => return None,
    })
}

#[derive(Debug, PartialEq, Serialize)]
pub struct FileReference {
    pub tag: u8,
    pub global: u8,
    pub file_id: String,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Mask {
    pub file_id: String,
    /// Eight bits for `Game.Unity` (`Mask.Read` reads a single byte elsewhere).
    pub bits: [u8; 8],
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Mask16 {
    pub file_id: String,
    pub material_match_mask: [u8; 16],
}

/// The seventeen `Game.Unity` booleans, in serialized order.
#[derive(Debug, PartialEq, Serialize)]
pub struct Flags {
    pub z_write_disabled_opaque: bool,
    pub z_write_disabled_alpha: bool,
    pub z_test_disabled: bool,
    pub layer_material: bool,
    pub layer_override_albedo: bool,
    pub layer_override_normal: bool,
    pub layer_override_specular: bool,
    pub pre_transparent: bool,
    pub flip_normal_on_back_face: bool,
    pub shadow_caster_opaque: bool,
    pub shadow_caster_alpha: bool,
    pub is_atlas_material: bool,
    pub depth_read_write: bool,
    pub alpha_test_enabled: bool,
    pub two_sided: bool,
    pub material_disabled: bool,
    pub mask_write_disabled: bool,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct TextureSelector {
    pub specification_method: i32,
    pub map_type: i32,
    pub frame_number: i32,
    pub texture_set: FileReference,
    pub texture_map: FileReference,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct UvTransform {
    pub scale_u: f32,
    pub scale_v: f32,
    pub translation_u: f32,
    pub translation_v: f32,
    pub translation_speed_u: f32,
    pub translation_speed_v: f32,
    pub rotation: f32,
    pub rotation_speed: f32,
    pub animate_translation: bool,
    pub animate_rotation: bool,
    pub unk01: u8,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "class", rename_all = "snake_case")]
pub enum ObjectData {
    TextureSelector(TextureSelector),
    UvTransform(UvTransform),
}

/// A `BaseTypes.Object` parameter value: an inline class instance.
#[derive(Debug, PartialEq, Serialize)]
pub struct ObjectValue {
    pub class_id: String,
    pub class_hash: String,
    /// `None` when the peeked class hash already matched `DataType`. When
    /// present, the raw leading byte the reference reader consumes as
    /// `IsManaged`; its native meaning is unresolved.
    pub managed_flag: Option<u8>,
    #[serde(flatten)]
    pub data: ObjectData,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ParameterValue {
    Bool(bool),
    Char(u8),
    Signed8(i8),
    Unsigned8(u8),
    Signed16(i16),
    Unsigned16(u16),
    Signed32(i32),
    Unsigned32(u32),
    Signed64(i64),
    Unsigned64(u64),
    Float(f32),
    Vector2([f32; 2]),
    Vector3([f32; 3]),
    Vector4([f32; 4]),
    Quaternion([f32; 4]),
    Matrix33([f32; 9]),
    Matrix44([f32; 16]),
    ObjectId(String),
    Handle(String),
    Enum(i32),
    Object(Box<ObjectValue>),
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Parameter {
    pub name_hash: String,
    /// The concrete class hash for object parameters; zero for value types.
    pub data_type: String,
    /// The registry type code that selects the value layout.
    pub kind: String,
    pub unk: u32,
    pub value: ParameterValue,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Material {
    pub file_id: String,
    pub template: FileReference,
    pub texture_set: FileReference,
    pub mask: Mask,
    pub blend_mode: i32,
    pub alpha_display_mode: i32,
    pub alpha_test_value: u8,
    pub flags: Flags,
    pub backface_material: FileReference,
    pub match_mask: Mask16,
    pub category: i32,
    pub parameters: Vec<Parameter>,
}

impl Material {
    /// First selector for a `TextureMapType` value, in parameter order.
    pub fn texture_selector(&self, map_type: u32) -> Option<&TextureSelector> {
        self.parameters.iter().find_map(|p| match &p.value {
            ParameterValue::Object(o) => match &o.data {
                ObjectData::TextureSelector(s) if s.map_type as u32 == map_type => Some(s),
                _ => None,
            },
            _ => None,
        })
    }
}

/// The twelve `Game.Unity` texture roles, in serialized order.
#[derive(Debug, PartialEq, Serialize)]
pub struct TextureSet {
    pub file_id: String,
    pub diffuse: FileReference,
    pub normal: FileReference,
    pub specular: FileReference,
    pub offset_bump: FileReference,
    pub emissive: FileReference,
    pub transmission: FileReference,
    pub occlusion: FileReference,
    pub mask1: FileReference,
    pub mask2: FileReference,
    pub cookie: FileReference,
    pub env_lighting: FileReference,
    pub generic: FileReference,
    /// Byte between the references and `source_id` that the reference reader
    /// skips without interpretation. Zero in every observed ACU texture set.
    pub source_prefix: u8,
    pub source_id: String,
}

fn reference(r: &mut Reader<'_>) -> Result<FileReference> {
    let tag = r.u8()?;
    let global = r.u8()?;
    let id = r.u64()?;
    // IsGlobal is a serialized byte, not a boolean: the installed throwing
    // blade material's texture-set reference carries 4 (ACU-RE-0013).
    if !matches!(tag, 1 | 3) || (tag == 3 && id != 0) {
        return invalid(format!(
            "unsupported material file reference at {}: tag={tag} global={global} id={id:016x}",
            r.position - 10
        ));
    }
    Ok(FileReference {
        tag,
        global,
        file_id: format!("{id:016x}"),
    })
}

fn class_header(r: &mut Reader<'_>, expected: u32) -> Result<String> {
    let id = format!("{:016x}", r.u64()?);
    let hash = r.u32()?;
    if hash != expected {
        return invalid(format!(
            "unexpected class hash {hash:#010x}, expected {expected:#010x}"
        ));
    }
    Ok(id)
}

fn count(r: &mut Reader<'_>, limit: usize, width: usize) -> Result<usize> {
    let n = r.u32()? as usize;
    if n > i32::MAX as usize || n > limit || n > r.remaining() / width {
        return invalid("material count exceeds input or configured limit");
    }
    Ok(n)
}

fn scalar(r: &mut Reader<'_>) -> Result<f32> {
    Ok(f32::from_le_bytes(r.take(4)?.try_into()?))
}

fn vector<const N: usize>(r: &mut Reader<'_>) -> Result<[f32; N]> {
    let mut values = [0.; N];
    for v in &mut values {
        *v = scalar(r)?;
    }
    Ok(values)
}

/// Registry codes from the reference tool's `PropertyRegistry`. A parameter's
/// `kind` selects the layout; `data_type` names the concrete class for objects.
fn registered(code: u32) -> bool {
    matches!(
        code,
        0x00_0000
            | 0x01_0000
            | 0x02_0000
            | 0x03_0000
            | 0x04_0000
            | 0x05_0000
            | 0x06_0000
            | 0x07_0000
            | 0x08_0000
            | 0x09_0000
            | 0x0a_0000
            | 0x0b_0000
            | 0x0c_0000
            | 0x0d_0000
            | 0x0e_0000
            | 0x0f_0000
            | 0x10_0000
            | 0x11_0000
            | 0x12_0000
            | 0x13_0000
            | 0x14_0000
            | 0x15_0000
            | 0x16_0000
            | 0x19_0000
            | 0x1a_0000
            | 0x1b_0000
            | 0x1c_0000
    )
}

fn object_value(r: &mut Reader<'_>, data_type: u32) -> Result<ObjectValue> {
    // The reference reader peeks the class ID and hash, and consumes a leading
    // byte only when that hash differs from the declared data type.
    let start = r.position;
    let _ = r.u64()?;
    let peeked = r.u32()?;
    r.position = start;
    let managed_flag = if peeked == data_type {
        None
    } else {
        Some(r.u8()?)
    };
    let class_id = format!("{:016x}", r.u64()?);
    let class_hash = r.u32()?;
    if class_hash != data_type {
        return invalid(format!(
            "object class hash {class_hash:#010x} disagrees with data type {data_type:#010x}"
        ));
    }
    let data = match data_type {
        CLASS_TEXTURE_SELECTOR => ObjectData::TextureSelector(TextureSelector {
            specification_method: r.i32()?,
            map_type: r.i32()?,
            frame_number: r.i32()?,
            texture_set: reference(r)?,
            texture_map: reference(r)?,
        }),
        CLASS_UV_TRANSFORM => ObjectData::UvTransform(UvTransform {
            scale_u: scalar(r)?,
            scale_v: scalar(r)?,
            translation_u: scalar(r)?,
            translation_v: scalar(r)?,
            translation_speed_u: scalar(r)?,
            translation_speed_v: scalar(r)?,
            rotation: scalar(r)?,
            rotation_speed: scalar(r)?,
            animate_translation: r.u8()? != 0,
            animate_rotation: r.u8()? != 0,
            unk01: r.u8()?,
        }),
        other => {
            return invalid(format!(
                "unsupported material object class {other:#010x} at {}",
                start
            ));
        }
    };
    Ok(ObjectValue {
        class_id,
        class_hash: format!("{class_hash:08x}"),
        managed_flag,
        data,
    })
}

fn parameter_value(r: &mut Reader<'_>, data_type: u32, kind: u32) -> Result<ParameterValue> {
    let resolved = if registered(kind) {
        kind
    } else if registered(data_type) {
        data_type
    } else {
        return invalid(format!(
            "unsupported material parameter type {kind:#010x} datatype {data_type:#010x}"
        ));
    };
    Ok(match resolved {
        0x00_0000 => ParameterValue::Bool(r.u8()? != 0),
        0x01_0000 => ParameterValue::Char(r.u8()?),
        0x02_0000 => ParameterValue::Signed8(r.u8()? as i8),
        0x03_0000 => ParameterValue::Unsigned8(r.u8()?),
        0x04_0000 => ParameterValue::Signed16(r.i16()?),
        0x05_0000 => ParameterValue::Unsigned16(r.u16()?),
        0x06_0000 => ParameterValue::Signed32(r.i32()?),
        0x07_0000 => ParameterValue::Unsigned32(r.u32()?),
        0x08_0000 => ParameterValue::Signed64(r.i64()?),
        0x09_0000 => ParameterValue::Unsigned64(r.u64()?),
        0x0a_0000 => ParameterValue::Float(scalar(r)?),
        0x0b_0000 => ParameterValue::Vector2(vector(r)?),
        0x0c_0000 => ParameterValue::Vector3(vector(r)?),
        0x0d_0000 => ParameterValue::Vector4(vector(r)?),
        0x0e_0000 => ParameterValue::Quaternion(vector(r)?),
        0x0f_0000 => ParameterValue::Matrix33(vector(r)?),
        0x10_0000 => ParameterValue::Matrix44(vector(r)?),
        0x11_0000 => ParameterValue::ObjectId(format!("{:016x}", r.u64()?)),
        0x12_0000 => {
            let _ = r.u8()?;
            ParameterValue::Handle(format!("{:016x}", r.u64()?))
        }
        0x13_0000 => ParameterValue::Object(Box::new(object_value(r, data_type)?)),
        0x19_0000 => ParameterValue::Enum(r.i32()?),
        other => {
            return invalid(format!(
                "unsupported material parameter type {other:#010x} datatype {data_type:#010x}"
            ));
        }
    })
}

/// Decode one `Material` resource. The payload is a resource root wrapper, the
/// class identity, the fixed header and every declared parameter; any leftover
/// byte or unsupported parameter type is an error.
pub fn parse(payload: &[u8], expected_id: &str, limits: Limits) -> Result<Material> {
    if payload.len() > limits.max_decoded {
        return invalid("material payload exceeds configured limit");
    }
    let mut r = Reader::new(payload);
    if r.u8()? != 1 {
        return invalid("unsupported material root wrapper");
    }
    let file_id = class_header(&mut r, CLASS_MATERIAL)?;
    if file_id != expected_id {
        return invalid("material root identity mismatch");
    }
    let template = reference(&mut r)?;
    let texture_set = reference(&mut r)?;
    let mask = Mask {
        file_id: class_header(&mut r, CLASS_MASK)?,
        bits: r.take(8)?.try_into()?,
    };
    let blend_mode = r.i32()?;
    let alpha_display_mode = r.i32()?;
    let alpha_test_value = r.u8()?;
    let flags = Flags {
        z_write_disabled_opaque: r.u8()? != 0,
        z_write_disabled_alpha: r.u8()? != 0,
        z_test_disabled: r.u8()? != 0,
        layer_material: r.u8()? != 0,
        layer_override_albedo: r.u8()? != 0,
        layer_override_normal: r.u8()? != 0,
        layer_override_specular: r.u8()? != 0,
        pre_transparent: r.u8()? != 0,
        flip_normal_on_back_face: r.u8()? != 0,
        shadow_caster_opaque: r.u8()? != 0,
        shadow_caster_alpha: r.u8()? != 0,
        is_atlas_material: r.u8()? != 0,
        depth_read_write: r.u8()? != 0,
        alpha_test_enabled: r.u8()? != 0,
        two_sided: r.u8()? != 0,
        material_disabled: r.u8()? != 0,
        mask_write_disabled: r.u8()? != 0,
    };
    let backface_material = reference(&mut r)?;
    let match_mask = Mask16 {
        file_id: class_header(&mut r, CLASS_MASK16)?,
        material_match_mask: r.take(16)?.try_into()?,
    };
    let category = r.i32()?;
    let n = count(&mut r, limits.max_resources, 16)?;
    let mut parameters = Vec::with_capacity(n);
    for _ in 0..n {
        let name_hash = format!("{:08x}", r.u32()?);
        let data_type = r.u32()?;
        let kind = r.u32()?;
        let unk = r.u32()?;
        let value = parameter_value(&mut r, data_type, kind)?;
        parameters.push(Parameter {
            name_hash,
            data_type: format!("{data_type:08x}"),
            kind: format!("{kind:08x}"),
            unk,
            value,
        });
    }
    if r.remaining() != 0 {
        return invalid(format!("trailing material payload bytes at {}", r.position));
    }
    Ok(Material {
        file_id,
        template,
        texture_set,
        mask,
        blend_mode,
        alpha_display_mode,
        alpha_test_value,
        flags,
        backface_material,
        match_mask,
        category,
        parameters,
    })
}

/// Decode one `TextureSet` resource (`Game.Unity` role list).
pub fn parse_texture_set(payload: &[u8], expected_id: &str, limits: Limits) -> Result<TextureSet> {
    if payload.len() > limits.max_decoded {
        return invalid("texture set payload exceeds configured limit");
    }
    let mut r = Reader::new(payload);
    if r.u8()? != 1 {
        return invalid("unsupported texture set root wrapper");
    }
    let file_id = class_header(&mut r, CLASS_TEXTURE_SET)?;
    if file_id != expected_id {
        return invalid("texture set root identity mismatch");
    }
    let set = TextureSet {
        diffuse: reference(&mut r)?,
        normal: reference(&mut r)?,
        specular: reference(&mut r)?,
        offset_bump: reference(&mut r)?,
        emissive: reference(&mut r)?,
        transmission: reference(&mut r)?,
        occlusion: reference(&mut r)?,
        mask1: reference(&mut r)?,
        mask2: reference(&mut r)?,
        cookie: reference(&mut r)?,
        env_lighting: reference(&mut r)?,
        generic: reference(&mut r)?,
        source_prefix: r.u8()?,
        source_id: format!("{:016x}", r.u64()?),
        file_id,
    };
    if r.remaining() != 0 {
        return invalid(format!(
            "trailing texture set payload bytes at {}",
            r.position
        ));
    }
    Ok(set)
}
