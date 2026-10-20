//! In-memory ACU runtime asset readers. No dependency on extracted game data.
#![forbid(unsafe_code)]

pub mod animation;
pub mod catalog;
pub mod control;
pub mod forge;
pub mod frame;
pub mod material;
pub mod mesh;
pub mod package;
pub mod prefetch;
pub mod reader;
pub mod skeleton;
pub mod texture;
pub mod vertex;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(std::io::Error::new(std::io::ErrorKind::InvalidData, message.into()).into())
}

pub fn sha256(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(data))
}

/// Defensive limits, not recovered native engine constants.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_entries: usize,
    pub max_tables: usize,
    pub max_stored: usize,
    pub max_decoded: usize,
    pub max_resources: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 1_000_000,
            max_tables: 4096,
            max_stored: 256 * 1024 * 1024,
            max_decoded: 512 * 1024 * 1024,
            max_resources: 1_000_000,
        }
    }
}
