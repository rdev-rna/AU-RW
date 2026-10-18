//! Runtime metadata index over explicitly selected original archives.
//! This is a reconstruction choice, not a recovered native lookup/patch policy.
use crate::{Limits, Result, control, forge::Forge, invalid, package::Package};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

/// Aggregate allocation/work limits, distinct from per-archive/parser limits.
#[derive(Clone, Copy, Debug)]
pub struct CatalogLimits {
    pub max_archives: usize,
    pub max_entries: usize,
    pub max_candidates: usize,
    pub max_metadata_bytes: usize,
    pub max_issues: usize,
}

impl Default for CatalogLimits {
    fn default() -> Self {
        Self {
            max_archives: 128,
            max_entries: 1_000_000,
            max_candidates: 4_000_000,
            max_metadata_bytes: 512 * 1024 * 1024,
            max_issues: 4096,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Candidate {
    pub file_id: String,
    pub archive_index: usize,
    pub entry_index: usize,
    pub package_file_id: String,
    pub resource_index: usize,
    pub record_offset: usize,
    pub record_length: usize,
    pub metadata_tags: Vec<u16>,
}

#[derive(Debug, Serialize)]
pub struct IndexIssue {
    pub archive_index: usize,
    pub entry_index: Option<usize>,
    pub error: String,
}

#[derive(Default, Debug, Serialize)]
pub struct CatalogReport {
    pub archives: usize,
    pub entries: usize,
    pub skipped_control_entries: usize,
    pub indexed_packages: usize,
    pub resource_candidates: usize,
    pub decoded_metadata_bytes: usize,
    pub issues: Vec<IndexIssue>,
}

pub struct Catalog {
    archives: Vec<Forge>,
    candidates: BTreeMap<String, Vec<Candidate>>,
    report: CatalogReport,
}

/// Owns a single lazily decoded package; all payloads stay in its memory buffer.
pub struct LoadedResource {
    pub package: Package,
    pub resource_index: usize,
    pub candidate: Candidate,
}

impl LoadedResource {
    pub fn resource(&self) -> &crate::package::Resource {
        &self.package.resources[self.resource_index]
    }

    pub fn payload(&self) -> &[u8] {
        &self.package.bytes[self.resource().payload.clone()]
    }
}

fn add_bounded(value: &mut usize, amount: usize, limit: usize, label: &str) -> Result<()> {
    *value = value
        .checked_add(amount)
        .ok_or("catalog aggregate overflow")?;
    if *value > limit {
        return invalid(format!("catalog {label} limit exceeded"));
    }
    Ok(())
}

impl Catalog {
    /// Index all resource metadata in caller-selected archives. Selection order
    /// identifies carriers only; it never selects a winner among duplicate IDs.
    /// Index issues remain visible, and make `index_complete()` false.
    pub fn open<P: AsRef<Path>>(
        paths: impl IntoIterator<Item = P>,
        limits: Limits,
        catalog_limits: CatalogLimits,
    ) -> Result<Self> {
        let mut archives = Vec::new();
        let mut entry_count = 0usize;
        for path in paths {
            if archives.len() >= catalog_limits.max_archives {
                return invalid("catalog archive limit exceeded");
            }
            let archive = Forge::open(path, limits)?;
            add_bounded(
                &mut entry_count,
                archive.entries.len(),
                catalog_limits.max_entries,
                "entry",
            )?;
            archives.push(archive);
        }
        if archives.is_empty() {
            return invalid("catalog requires explicitly selected archives");
        }
        let mut result = Self {
            archives,
            candidates: BTreeMap::new(),
            report: CatalogReport {
                entries: entry_count,
                ..CatalogReport::default()
            },
        };
        result.report.archives = result.archives.len();
        for archive_index in 0..result.archives.len() {
            if result.archives[archive_index].declared_count
                != result.archives[archive_index].entries.len()
            {
                result.issue(
                    archive_index,
                    None,
                    "declared/reachable archive entry counts disagree".into(),
                    catalog_limits,
                )?;
            }
            for entry_index in 0..result.archives[archive_index].entries.len() {
                let package_file_id = result.archives[archive_index].entries[entry_index]
                    .file_id
                    .clone();
                if control::is_control(&package_file_id) {
                    result.report.skipped_control_entries += 1;
                    continue;
                }
                let metadata = match result.archives[archive_index].read_metadata(entry_index) {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        result.issue(
                            archive_index,
                            Some(entry_index),
                            error.to_string(),
                            catalog_limits,
                        )?;
                        continue;
                    }
                };
                let metadata_bytes = metadata
                    .records
                    .iter()
                    .try_fold(2usize, |sum, record| {
                        sum.checked_add(14)?
                            .checked_add(record.metadata_tags.len().checked_mul(2)?)
                    })
                    .and_then(|sum| sum.checked_add(metadata.metadata_extension.len()))
                    .ok_or("catalog metadata size overflow")?;
                add_bounded(
                    &mut result.report.decoded_metadata_bytes,
                    metadata_bytes,
                    catalog_limits.max_metadata_bytes,
                    "metadata byte",
                )?;
                if !metadata.metadata_extension.is_empty() {
                    result.issue(
                        archive_index,
                        Some(entry_index),
                        "unsupported package metadata extension".into(),
                        catalog_limits,
                    )?;
                    continue;
                }
                add_bounded(
                    &mut result.report.resource_candidates,
                    metadata.records.len(),
                    catalog_limits.max_candidates,
                    "candidate",
                )?;
                for (resource_index, record) in metadata.records.into_iter().enumerate() {
                    let candidate = Candidate {
                        file_id: record.file_id,
                        archive_index,
                        entry_index,
                        package_file_id: package_file_id.clone(),
                        resource_index,
                        record_offset: record.record_offset,
                        record_length: record.record_length,
                        metadata_tags: record.metadata_tags,
                    };
                    result
                        .candidates
                        .entry(candidate.file_id.clone())
                        .or_default()
                        .push(candidate);
                }
                result.report.indexed_packages += 1;
            }
        }
        Ok(result)
    }

    fn issue(
        &mut self,
        archive_index: usize,
        entry_index: Option<usize>,
        error: String,
        limits: CatalogLimits,
    ) -> Result<()> {
        if self.report.issues.len() >= limits.max_issues {
            return invalid("catalog issue limit exceeded");
        }
        self.report.issues.push(IndexIssue {
            archive_index,
            entry_index,
            error,
        });
        Ok(())
    }

    pub fn report(&self) -> &CatalogReport {
        &self.report
    }

    /// Completeness concerns only metadata indexing of selected archives, not
    /// payload validity, typed asset coverage or native patch precedence.
    pub fn index_complete(&self) -> bool {
        self.report.issues.is_empty()
    }

    pub fn archive_path(&self, archive_index: usize) -> Option<&Path> {
        self.archives
            .get(archive_index)
            .map(|archive| archive.path.as_path())
    }

    pub fn candidates(&self, file_id: &str) -> Result<&[Candidate]> {
        if file_id.len() != 16 || !file_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return invalid("resource ID must contain exactly sixteen hexadecimal digits");
        }
        Ok(self
            .candidates
            .get(&file_id.to_ascii_lowercase())
            .map(Vec::as_slice)
            .unwrap_or(&[]))
    }

    /// Convenience only for an unambiguous index. No archive filename, ordering,
    /// metadata tag or record position is treated as a patch-precedence rule.
    pub fn unique(&self, file_id: &str) -> Result<&Candidate> {
        let candidates = self.candidates(file_id)?;
        if candidates.len() != 1 {
            return invalid(format!(
                "resource {file_id} has {} candidates; choose explicitly",
                candidates.len()
            ));
        }
        Ok(&candidates[0])
    }

    /// Decode only the requested package, then verify its resource identity and
    /// metadata position against the chosen candidate. Unsupported asset types
    /// remain unsupported even when their envelope/payload bytes can be loaded.
    pub fn load(&mut self, candidate: &Candidate) -> Result<LoadedResource> {
        if !self.candidates(&candidate.file_id)?.contains(candidate) {
            return invalid("resource candidate does not belong to this catalog");
        }
        let archive = self
            .archives
            .get_mut(candidate.archive_index)
            .ok_or("candidate archive outside catalog")?;
        let entry = archive
            .entries
            .get(candidate.entry_index)
            .ok_or("candidate entry outside archive")?;
        if entry.file_id != candidate.package_file_id {
            return invalid("candidate package identity changed");
        }
        let package = archive.read_package(candidate.entry_index)?;
        let resource = package
            .resources
            .get(candidate.resource_index)
            .ok_or("candidate resource outside package")?;
        if resource.file_id != candidate.file_id
            || resource.record_offset != candidate.record_offset
            || resource.record_length != candidate.record_length
            || resource.metadata_tags != candidate.metadata_tags
            || !package.metadata_extension.is_empty()
        {
            return invalid("loaded resource identity/metadata differs from catalog");
        }
        Ok(LoadedResource {
            package,
            resource_index: candidate.resource_index,
            candidate: candidate.clone(),
        })
    }
}
