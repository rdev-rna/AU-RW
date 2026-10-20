//! Global Forge metadata. Layout and reference field names: ACU-RE-0004.
use crate::{Limits, Result, invalid, reader::Reader};
use serde::Serialize;

pub const META_ID: &str = "0000000000000010";
pub const PREFETCH_ID: &str = "0000000000000091";

pub enum ArchiveControl {
    Metadata(GlobalMetadata),
    Prefetch(crate::prefetch::PrefetchIndex),
}

pub struct DecodedControl {
    pub data: ArchiveControl,
    pub frames: Vec<crate::frame::FrameInfo>,
    pub decoded_length: usize,
    pub decoded_sha256: String,
}

pub fn is_control(file_id: &str) -> bool {
    matches!(file_id, META_ID | PREFETCH_ID)
}

/// Read one evidenced control variant without treating it as a resource package.
pub fn decode(file_id: &str, raw: &[u8], limits: Limits) -> Result<DecodedControl> {
    if raw.len() > limits.max_stored {
        return invalid("archive control stored byte limit exceeded");
    }
    match file_id {
        META_ID => Ok(DecodedControl {
            data: ArchiveControl::Metadata(parse_metadata(raw, limits)?),
            frames: Vec::new(),
            decoded_length: raw.len(),
            decoded_sha256: crate::sha256(raw),
        }),
        PREFETCH_ID => {
            let mut r = Reader::new(raw);
            let (bytes, info) = crate::frame::decode(&mut r, limits)?;
            if r.remaining() != 0 {
                return invalid("trailing bytes after prefetch outer frame");
            }
            Ok(DecodedControl {
                data: ArchiveControl::Prefetch(crate::prefetch::parse(&bytes, limits)?),
                frames: vec![info],
                decoded_length: bytes.len(),
                decoded_sha256: crate::sha256(&bytes),
            })
        }
        _ => invalid("entry ID is not a supported archive control"),
    }
}

impl DecodedControl {
    /// Bounded report descriptor; bulk dependency/signature arrays stay in the runtime API.
    pub fn summary(&self) -> serde_json::Value {
        use serde_json::json;
        let mut value = match &self.data {
            ArchiveControl::Metadata(meta) => json!({
                "kind":"global_metadata", "version":meta.version,
                "attributes":meta.attributes.iter().map(|a| {
                    let mut item = json!({"key":a.key,"name":a.name,"offset":a.offset});
                    match &a.value {
                        MetaValue::U32(v) => { item["u32"] = json!(v); },
                        MetaValue::Text(v) => { item["text_length"] = json!(v.len()); },
                        MetaValue::U64Array(v) => { item["u64_count"] = json!(v.len()); },
                    }
                    item
                }).collect::<Vec<_>>()
            }),
            ArchiveControl::Prefetch(index) => json!({
                "kind":"prefetch_index", "entries":index.entries.len(),
                "unique_blocks":index.blocks.len(), "blob_length":index.blob_length,
                "dependencies":index.blocks.iter().map(|b|b.dependencies.len()).sum::<usize>(),
                "filters":index.blocks.iter().map(|b|b.filters.len()).sum::<usize>()
            }),
        };
        value["decoded_length"] = json!(self.decoded_length);
        value["decoded_sha256"] = json!(self.decoded_sha256);
        value["frames"] = json!(self.frames);
        value
    }
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "kind", content = "value")]
pub enum MetaValue {
    U32(u32),
    Text(String),
    U64Array(Vec<String>),
}

#[derive(Debug, Serialize)]
pub struct Attribute {
    pub key: u32,
    pub name: &'static str,
    pub offset: usize,
    pub value: MetaValue,
}

#[derive(Debug, Serialize)]
pub struct GlobalMetadata {
    pub version: u32,
    pub attributes: Vec<Attribute>,
}

fn attribute_name(key: u32) -> Result<&'static str> {
    Ok(match key {
        1 => "GUID",
        2 => "DataChangeList",
        3 => "ClientName",
        4 => "DataBranchPath",
        5 => "SndBranchPath",
        7 => "PreviousGUID",
        10 => "DataControlVersion",
        11 => "DataServer",
        12 => "CodeChangeList",
        13 => "SoundChangeList",
        14 => "CodeVersion",
        15 => "BigFileDataType",
        16 => "DatabaseIndexingGUID",
        19 => "FileGUID",
        21 => "ContentPackagesList",
        24 => "ImportFlag",
        30 => "EmbeddedPackageIDs",
        31 => "IntegrationModeActive",
        32 => "DLCVersion",
        33 => "FastLoadSignatures",
        999 => "MPFileTag",
        _ => return invalid(format!("unsupported global metadata key {key}")),
    })
}

pub fn parse_metadata(raw: &[u8], limits: Limits) -> Result<GlobalMetadata> {
    if raw.len() > limits.max_decoded {
        return invalid("global metadata byte limit exceeded");
    }
    let mut r = Reader::new(raw);
    let version = r.u32()?;
    if version != 1 {
        return invalid("unsupported global metadata version");
    }
    let mut attributes = Vec::new();
    loop {
        let offset = r.position;
        let key = r.u32()?;
        if key == 0 {
            break;
        }
        if attributes.len() >= limits.max_resources {
            return invalid("global metadata attribute limit exceeded");
        }
        let name = attribute_name(key)?;
        let kind = r.u32()?;
        let value = match kind {
            0 => MetaValue::U32(r.u32()?),
            1 => {
                let size = r.u32()? as usize;
                let text = std::str::from_utf8(r.take(size)?)?;
                if r.u8()? != 0 {
                    return invalid("global metadata string terminator is not zero");
                }
                MetaValue::Text(text.to_owned())
            }
            6 => {
                let size = r.u32()? as usize;
                if !size.is_multiple_of(8) || size / 8 > limits.max_resources {
                    return invalid("global metadata array byte length invalid/over limit");
                }
                let mut array = Reader::new(r.take(size)?);
                let mut values = Vec::with_capacity(size / 8);
                while array.remaining() != 0 {
                    values.push(format!("{:016x}", array.u64()?));
                }
                MetaValue::U64Array(values)
            }
            _ => return invalid(format!("unsupported global metadata value type {kind}")),
        };
        attributes.push(Attribute {
            key,
            name,
            offset,
            value,
        });
    }
    if r.remaining() != 0 {
        return invalid("trailing bytes after global metadata terminator");
    }
    Ok(GlobalMetadata {
        version,
        attributes,
    })
}
