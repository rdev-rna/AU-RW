//! Temporary timing probe for catalog indexing over selected archives.
use acu_assets::{
    Limits,
    catalog::{Catalog, CatalogLimits},
    forge::Forge,
};
use std::time::Instant;

fn main() -> acu_assets::Result<()> {
    let game =
        std::path::PathBuf::from(std::env::var("ACU_GAME_DIR").map_err(|_| "set ACU_GAME_DIR")?);
    let limits = Limits::default();
    let names: Vec<String> = std::env::args().skip(1).collect();
    for name in &names {
        let path = game.join(name);
        let start = Instant::now();
        let mut forge = Forge::open(&path, limits)?;
        let opened = start.elapsed();
        let start = Instant::now();
        let mut entries = 0usize;
        let mut bytes = 0usize;
        for i in 0..forge.entries.len() {
            if let Ok(metadata) = forge.read_metadata(i) {
                entries += 1;
                bytes += metadata
                    .records
                    .iter()
                    .map(|r| r.metadata_tags.len() * 2 + 14)
                    .sum::<usize>()
                    + metadata.metadata_extension.len();
            }
        }
        println!(
            "{name}: open {opened:?}, metadata {entries} entries in {:?} ({bytes} bytes)",
            start.elapsed()
        );
    }
    let paths: Vec<std::path::PathBuf> = names.iter().map(|n| game.join(n)).collect();
    let start = Instant::now();
    let catalog = Catalog::open(paths, limits, CatalogLimits::default())?;
    println!(
        "Catalog::open {} archives in {:?}; candidates {}; issues {}; complete {}",
        catalog.report().archives,
        start.elapsed(),
        catalog.report().resource_candidates,
        catalog.report().issues.len(),
        catalog.index_complete()
    );
    Ok(())
}
