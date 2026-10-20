//! Plaintext v27 container. Field provenance: ACU-RE-0001.
use crate::{Limits, Result, invalid, reader::Reader};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub index: usize,
    pub record_offset: u64,
    pub offset: u64,
    pub file_id: String,
    pub stored_length: u32,
}

pub struct Forge {
    pub path: PathBuf,
    file: File,
    length: u64,
    pub declared_count: usize,
    pub entries: Vec<Entry>,
    pub limits: Limits,
}

fn read_range(
    file: &mut File,
    length: u64,
    offset: u64,
    size: usize,
    limit: usize,
) -> Result<Vec<u8>> {
    if size > limit || offset > length || size as u64 > length - offset {
        return invalid(format!(
            "invalid/over-limit range offset={offset}, size={size}, file={length}"
        ));
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut data = vec![0; size];
    file.read_exact(&mut data)?;
    Ok(data)
}

impl Forge {
    pub fn open(path: impl AsRef<Path>, limits: Limits) -> Result<Self> {
        let path = path.as_ref().to_owned();
        let mut file = File::open(&path)?;
        let length = file.metadata()?.len();
        let header = read_range(&mut file, length, 0, 0x41a, limits.max_stored)?;
        let mut r = Reader::new(&header);
        if r.take(9)? != b"scimitar\0" {
            return invalid("unsupported Forge magic");
        }
        if r.u32()? != 27 {
            return invalid("unsupported Forge version");
        }
        let dho = r.u64()?;
        let dh = read_range(&mut file, length, dho, 44, limits.max_stored)?;
        let mut r = Reader::new(&dh);
        let declared_count = r.u32()? as usize;
        if declared_count > limits.max_entries {
            return invalid("declared entry limit exceeded");
        }
        r.take(32)?;
        let mut table = r.u64()?;
        let mut seen = HashSet::new();
        let mut entries = Vec::new();
        while table != 0 && table != u64::MAX {
            if !seen.insert(table) {
                return invalid("Forge table cycle");
            }
            if seen.len() > limits.max_tables {
                return invalid("Forge table limit exceeded");
            }
            let raw = read_range(&mut file, length, table, 48, limits.max_stored)?;
            let mut r = Reader::new(&raw);
            let count = r.u32()? as usize;
            r.take(4)?;
            let array = r.u64()?;
            let next = r.u64()?;
            if count > limits.max_entries - entries.len() {
                return invalid("Forge entry limit exceeded");
            }
            let size = count
                .checked_mul(20)
                .ok_or("entry table multiplication overflow")?;
            let raw = read_range(&mut file, length, array, size, limits.max_stored)?;
            let mut r = Reader::new(&raw);
            for i in 0..count {
                let offset = r.u64()?;
                let fid = r.u64()?;
                let stored_length = r.u32()?;
                if offset > length || stored_length as u64 > length - offset {
                    return invalid("Forge entry outside archive");
                }
                entries.push(Entry {
                    index: entries.len(),
                    record_offset: array + (i * 20) as u64,
                    offset,
                    file_id: format!("{fid:016x}"),
                    stored_length,
                });
            }
            table = next;
        }
        Ok(Self {
            path,
            file,
            length,
            declared_count,
            entries,
            limits,
        })
    }
    pub fn raw(&mut self, index: usize) -> Result<Vec<u8>> {
        let e = self
            .entries
            .get(index)
            .ok_or("entry index outside archive")?;
        read_range(
            &mut self.file,
            self.length,
            e.offset,
            e.stored_length as usize,
            self.limits.max_stored,
        )
    }

    /// Load a requested package directly from this open archive, decoding only in memory.
    pub fn read_package(&mut self, index: usize) -> Result<crate::package::Package> {
        crate::package::decode(&self.raw(index)?, self.limits)
    }

    pub fn read_global_metadata(&mut self, index: usize) -> Result<crate::control::GlobalMetadata> {
        if self
            .entries
            .get(index)
            .is_none_or(|e| e.file_id != crate::control::META_ID)
        {
            return invalid("entry is not global archive metadata");
        }
        crate::control::parse_metadata(&self.raw(index)?, self.limits)
    }

    pub fn read_control(&mut self, index: usize) -> Result<crate::control::DecodedControl> {
        let file_id = self
            .entries
            .get(index)
            .ok_or("entry index outside archive")?
            .file_id
            .clone();
        crate::control::decode(&file_id, &self.raw(index)?, self.limits)
    }

    /// Read only the first TOC metadata frame. The resource payload remains on disk.
    /// Unsupported frame modes and control IDs fail explicitly. ACU-RE-0002/0012.
    pub fn read_metadata(&mut self, index: usize) -> Result<crate::package::PackageMetadata> {
        let entry = self
            .entries
            .get(index)
            .ok_or("entry index outside archive")?
            .clone();
        if crate::control::is_control(&entry.file_id) || entry.stored_length < 19 {
            return invalid("entry does not contain a resource metadata frame");
        }
        let header = read_range(
            &mut self.file,
            self.length,
            entry.offset,
            19,
            self.limits.max_stored,
        )?;
        let mut r = Reader::new(&header);
        if r.u64()? != crate::frame::MAGIC {
            return invalid("unsupported metadata frame magic");
        }
        let version = r.u16()?;
        let algorithm = r.u8()?;
        let flags = r.u32()?;
        if version > 3 || algorithm > 1 || flags & 0x80000000 == 0 {
            return invalid("metadata-only read requires a supported TOC frame");
        }
        let count = r.u32()? as usize;
        let width = if version == 1 { 4usize } else { 8usize };
        if count > self.limits.max_resources {
            return invalid("metadata frame block limit exceeded");
        }
        let table_length = count
            .checked_mul(width)
            .ok_or("metadata block table overflow")?;
        let prefix_length = 19usize
            .checked_add(table_length)
            .ok_or("metadata prefix size overflow")?;
        if prefix_length > entry.stored_length as usize {
            return invalid("metadata block table outside entry");
        }
        let table = read_range(
            &mut self.file,
            self.length,
            entry.offset + 19,
            table_length,
            self.limits.max_stored,
        )?;
        let mut r = Reader::new(&table);
        let mut frame_length = prefix_length;
        let mut decoded_length = 0usize;
        for _ in 0..count {
            let (decoded, stored) = if version == 1 {
                (r.u16()? as usize, r.u16()? as usize)
            } else {
                (r.u32()? as usize, r.u32()? as usize)
            };
            decoded_length = decoded_length
                .checked_add(decoded)
                .ok_or("metadata decoded length overflow")?;
            frame_length = frame_length
                .checked_add(4)
                .and_then(|n| n.checked_add(stored))
                .ok_or("metadata frame length overflow")?;
        }
        if decoded_length > self.limits.max_decoded || frame_length > entry.stored_length as usize {
            return invalid("metadata frame outside entry/decoded limit");
        }
        let raw = read_range(
            &mut self.file,
            self.length,
            entry.offset,
            frame_length,
            self.limits.max_stored,
        )?;
        let mut r = Reader::new(&raw);
        let (metadata, _) = crate::frame::decode(&mut r, self.limits)?;
        if r.remaining() != 0 {
            return invalid("metadata-only frame size mismatch");
        }
        crate::package::parse_metadata(&metadata, self.limits)
    }
}

/// Enumerate original top-level/DLC carriers. Extracted folders and backups are excluded.
/// Directory symlinks are not followed. No filename-based patch priority is implied.
pub fn discover(root: &Path) -> Result<Vec<PathBuf>> {
    fn visit(root: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
        for item in std::fs::read_dir(root)? {
            let item = item?;
            let kind = item.file_type()?;
            let name = item.file_name().to_string_lossy().into_owned();
            if kind.is_file() && name.ends_with(".forge") {
                paths.push(item.path());
            } else if kind.is_dir() && name.starts_with("dlc_") {
                visit(&item.path(), paths)?;
            }
        }
        Ok(())
    }
    let mut paths = Vec::new();
    visit(root, &mut paths)?;
    paths.sort();
    if paths.is_empty() {
        return invalid("unavailable: no Forge archives found");
    }
    Ok(paths)
}
