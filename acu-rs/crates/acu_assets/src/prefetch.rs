//! ACU control ID 0x91. Layout and limits: ACU-RE-0006.
use crate::{Limits, Result, frame, invalid, reader::Reader};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct PrefetchIndex {
    pub entries: Vec<PrefetchEntry>,
    /// Shared ranges are decoded once; entries refer to this vector.
    pub blocks: Vec<PrefetchBlock>,
    pub blob_length: usize,
}

#[derive(Debug, Serialize)]
pub struct PrefetchEntry {
    pub file_id: String,
    pub stored_length: u32,
    pub offset: u32,
    pub compressed: bool,
    pub block_index: usize,
}

#[derive(Debug, Serialize)]
pub struct PrefetchBlock {
    pub decoded_length: usize,
    pub filters: Vec<DataLayerFilter>,
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Serialize)]
pub struct DataLayerFilter {
    pub flag: bool,
    pub actions: Vec<DataLayerAction>,
}

#[derive(Debug, Serialize)]
pub struct DataLayerAction {
    /// Retained numeric value; native action semantics remain unverified.
    pub action: u8,
    pub data_layer_id: String,
}

#[derive(Debug, Serialize)]
pub struct Dependency {
    pub file_id: String,
    /// Retained numeric value; native policy semantics remain unverified.
    pub policy: u8,
    pub filter_indices: Vec<u16>,
}

fn block(bytes: &[u8], limits: Limits) -> Result<PrefetchBlock> {
    let mut r = Reader::new(bytes);
    let count = r.u16()? as usize;
    if count > limits.max_resources || count > r.remaining() / 2 {
        return invalid("prefetch filter count exceeds bounds");
    }
    let mut filters = Vec::with_capacity(count);
    for _ in 0..count {
        let flags = r.u16()?;
        let count = (flags & 0x7fff) as usize;
        if count > limits.max_resources || count > r.remaining() / 9 {
            return invalid("prefetch action count exceeds bounds");
        }
        let mut actions = Vec::with_capacity(count);
        for _ in 0..count {
            actions.push(DataLayerAction {
                action: r.u8()?,
                data_layer_id: format!("{:016x}", r.u64()?),
            });
        }
        filters.push(DataLayerFilter {
            flag: flags & 0x8000 != 0,
            actions,
        });
    }
    let count = r.u16()? as usize;
    if count > limits.max_resources || count > r.remaining() / 11 {
        return invalid("prefetch dependency count exceeds bounds");
    }
    let mut dependencies = Vec::with_capacity(count);
    for _ in 0..count {
        let file_id = format!("{:016x}", r.u64()?);
        let policy = r.u8()?;
        let count = r.u16()? as usize;
        if count > limits.max_resources || count > r.remaining() / 2 {
            return invalid("prefetch filter index count exceeds bounds");
        }
        let mut filter_indices = Vec::with_capacity(count);
        for _ in 0..count {
            let index = r.u16()?;
            if index as usize >= filters.len() {
                return invalid("prefetch filter index out of range");
            }
            filter_indices.push(index);
        }
        dependencies.push(Dependency {
            file_id,
            policy,
            filter_indices,
        });
    }
    if r.remaining() != 0 {
        return invalid("unconsumed prefetch dependency bytes");
    }
    Ok(PrefetchBlock {
        decoded_length: bytes.len(),
        filters,
        dependencies,
    })
}

/// Parse the fully decoded outer frame; requires complete consumption.
pub fn parse(bytes: &[u8], limits: Limits) -> Result<PrefetchIndex> {
    if bytes.len() > limits.max_decoded {
        return invalid("prefetch input allocation limit exceeded");
    }
    let mut r = Reader::new(bytes);
    let count = r.u32()? as usize;
    if count > limits.max_entries || count > r.remaining() / 16 {
        return invalid("prefetch entry count exceeds bounds");
    }
    let mut entries = Vec::with_capacity(count);
    let mut ranges = BTreeMap::new();
    for _ in 0..count {
        let file_id = format!("{:016x}", r.u64()?);
        let flags = r.u32()?;
        let offset = r.u32()?;
        let stored_length = flags & 0x7fff_ffff;
        let compressed = flags & 0x8000_0000 != 0;
        if stored_length == 0 {
            return invalid("zero length prefetch block");
        }
        if let Some(previous) = ranges.insert((offset, stored_length), compressed)
            && previous != compressed
        {
            return invalid("inconsistent shared prefetch compression flag");
        }
        entries.push(PrefetchEntry {
            file_id,
            stored_length,
            offset,
            compressed,
            block_index: 0,
        });
    }
    let blob_length = r.u32()? as usize;
    let blob = r.take(blob_length)?;
    if r.remaining() != 0 {
        return invalid("unconsumed prefetch index bytes");
    }
    let mut blocks = Vec::with_capacity(ranges.len());
    let mut indices = BTreeMap::new();
    let mut end = 0usize;
    let mut decoded_total = 0usize;
    for ((offset, length), compressed) in ranges {
        if offset as usize != end {
            return invalid("prefetch ranges overlap or leave a gap");
        }
        end = end
            .checked_add(length as usize)
            .ok_or("prefetch range overflow")?;
        let stored = blob
            .get(offset as usize..end)
            .ok_or("prefetch range exceeds blob")?;
        if stored.len() > limits.max_stored {
            return invalid("prefetch stored block limit exceeded");
        }
        let decoded;
        let data = if compressed {
            let mut nested = Reader::new(stored);
            let remaining_budget = limits
                .max_decoded
                .checked_sub(decoded_total)
                .ok_or("prefetch decoded budget exceeded")?;
            (decoded, _) = frame::decode(
                &mut nested,
                Limits {
                    max_decoded: remaining_budget,
                    ..limits
                },
            )?;
            if nested.remaining() != 0 {
                return invalid("unconsumed nested prefetch frame");
            }
            decoded.as_slice()
        } else {
            stored
        };
        decoded_total = decoded_total
            .checked_add(data.len())
            .ok_or("prefetch decoded size overflow")?;
        if decoded_total > limits.max_decoded {
            return invalid("prefetch decoded budget exceeded");
        }
        indices.insert((offset, length), blocks.len());
        blocks.push(block(data, limits)?);
    }
    if end != blob.len() {
        return invalid("unreferenced prefetch blob bytes");
    }
    for entry in &mut entries {
        entry.block_index = indices[&(entry.offset, entry.stored_length)];
    }
    Ok(PrefetchIndex {
        entries,
        blocks,
        blob_length,
    })
}
