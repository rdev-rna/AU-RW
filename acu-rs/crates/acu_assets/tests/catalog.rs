use acu_assets::{
    Limits,
    catalog::{Catalog, CatalogLimits},
    frame,
};
use std::{
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "acu-catalog-{}-{}.forge",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.write_all(bytes).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).unwrap();
    }
}

fn raw_frame(data: &[u8]) -> Vec<u8> {
    let mut bytes = frame::MAGIC.to_le_bytes().to_vec();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.push(1);
    bytes.extend_from_slice(&0x80008000u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&(data.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&(data.len() as u16).to_le_bytes());
    let (mut a, mut b) = (0u32, 0u32);
    for byte in data {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    bytes.extend_from_slice(&(b << 16 | a).to_le_bytes());
    bytes.extend_from_slice(data);
    bytes
}

fn package(resources: &[(u64, &[u8], &[u16])]) -> Vec<u8> {
    let mut metadata = (resources.len() as u16).to_le_bytes().to_vec();
    let mut payload = Vec::new();
    for (id, data, tags) in resources {
        metadata.extend_from_slice(&id.to_le_bytes());
        metadata.extend_from_slice(&(14 + data.len() as u32).to_le_bytes());
        metadata.extend_from_slice(&(tags.len() as u16).to_le_bytes());
        for tag in *tags {
            metadata.extend_from_slice(&tag.to_le_bytes());
        }
        payload.extend_from_slice(&0x12345678u32.to_le_bytes());
        payload.extend_from_slice(&(data.len() as u32).to_le_bytes());
        payload.extend_from_slice(&1u32.to_le_bytes());
        payload.extend_from_slice(b"X\0");
        payload.extend_from_slice(data);
    }
    let mut bytes = raw_frame(&metadata);
    bytes.extend_from_slice(&raw_frame(&payload));
    bytes
}

fn archive(entries: &[(u64, Vec<u8>)], declared: usize) -> Vec<u8> {
    let mut bytes = vec![0; 0x600];
    bytes[..9].copy_from_slice(b"scimitar\0");
    bytes[9..13].copy_from_slice(&27u32.to_le_bytes());
    bytes[13..21].copy_from_slice(&0x41au64.to_le_bytes());
    bytes[0x41a..0x41e].copy_from_slice(&(declared as u32).to_le_bytes());
    bytes[0x43e..0x446].copy_from_slice(&0x446u64.to_le_bytes());
    bytes[0x446..0x44a].copy_from_slice(&(entries.len() as u32).to_le_bytes());
    bytes[0x44e..0x456].copy_from_slice(&0x476u64.to_le_bytes());
    bytes[0x456..0x45e].copy_from_slice(&u64::MAX.to_le_bytes());
    for (index, (id, data)) in entries.iter().enumerate() {
        let record = 0x476 + index * 20;
        let offset = bytes.len() as u64;
        bytes[record..record + 8].copy_from_slice(&offset.to_le_bytes());
        bytes[record + 8..record + 16].copy_from_slice(&id.to_le_bytes());
        bytes[record + 16..record + 20].copy_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(data);
    }
    bytes
}

#[test]
fn duplicate_resources_preserve_all_candidates_and_load_explicitly() {
    let first = Fixture::new(&archive(
        &[(100, package(&[(42, b"first", &[5]), (42, b"second", &[])]))],
        1,
    ));
    let second = Fixture::new(&archive(
        &[(
            200,
            package(&[(42, b"third", &[2, 3]), (43, b"unique", &[])]),
        )],
        1,
    ));
    let mut catalog = Catalog::open(
        [&first.0, &second.0],
        Limits::default(),
        CatalogLimits::default(),
    )
    .unwrap();
    assert!(catalog.index_complete());
    assert_eq!(catalog.report().resource_candidates, 4);
    assert_eq!(catalog.report().decoded_metadata_bytes, 66);
    let candidates = catalog.candidates("000000000000002A").unwrap().to_vec();
    assert_eq!(candidates.len(), 3);
    assert!(catalog.unique("000000000000002a").is_err());
    for (candidate, expected) in candidates
        .iter()
        .zip([b"first".as_slice(), b"second", b"third"])
    {
        assert_eq!(catalog.load(candidate).unwrap().payload(), expected);
    }
    assert_eq!(candidates[1].record_offset, 19);
    assert_eq!(candidates[2].metadata_tags, vec![2, 3]);
    let unique = catalog.unique("000000000000002b").unwrap().clone();
    assert_eq!(
        catalog.load(&unique).unwrap().resource().file_id,
        "000000000000002b"
    );
    let mut fabricated = unique;
    fabricated.resource_index = 99;
    assert!(catalog.load(&fabricated).is_err());
    assert!(catalog.candidates("42").is_err());
    assert!(catalog.candidates("zzzzzzzzzzzzzzzz").is_err());
    assert!(catalog.candidates("000000000000ffff").unwrap().is_empty());
    assert!(catalog.unique("000000000000ffff").is_err());
}

#[test]
fn index_does_not_decode_payload_and_load_still_checks_it() {
    let mut raw = package(&[(42, b"payload", &[])]);
    // One metadata record: first frame is 27 + 16 bytes. Corrupt the
    // second frame magic while retaining a valid independently framed table.
    raw[43] ^= 0x80;
    let fixture = Fixture::new(&archive(&[(100, raw)], 1));
    let mut catalog =
        Catalog::open([&fixture.0], Limits::default(), CatalogLimits::default()).unwrap();
    assert!(catalog.index_complete());
    let candidate = catalog.unique("000000000000002a").unwrap().clone();
    assert!(catalog.load(&candidate).is_err());
}

#[test]
fn loading_rechecks_resource_identity_after_indexing() {
    let fixture = Fixture::new(&archive(&[(100, package(&[(42, b"payload", &[])]))], 1));
    let mut catalog =
        Catalog::open([&fixture.0], Limits::default(), CatalogLimits::default()).unwrap();
    let candidate = catalog.unique("000000000000002a").unwrap().clone();
    // Same carrier and package positions, but the on-disk metadata identity
    // changes after indexing. A candidate cannot silently load another ID.
    std::fs::write(
        &fixture.0,
        archive(&[(100, package(&[(43, b"payload", &[])]))], 1),
    )
    .unwrap();
    let error = catalog
        .load(&candidate)
        .err()
        .expect("identity change must fail");
    assert!(error.to_string().contains("identity/metadata differs"));
}

#[test]
fn incomplete_indices_remain_visible_and_controls_are_explicit() {
    let fixture = Fixture::new(&archive(
        &[
            (
                0x10,
                b"control intentionally not decoded by catalog".to_vec(),
            ),
            (100, package(&[(42, b"valid", &[])])),
            (101, raw_frame(&[1, 0])), // Declared metadata record is absent.
        ],
        4,
    ));
    let mut catalog =
        Catalog::open([&fixture.0], Limits::default(), CatalogLimits::default()).unwrap();
    assert!(!catalog.index_complete());
    assert_eq!(catalog.report().skipped_control_entries, 1);
    assert_eq!(catalog.report().indexed_packages, 1);
    assert_eq!(catalog.report().issues.len(), 2);
    let candidate = catalog.unique("000000000000002a").unwrap().clone();
    assert_eq!(catalog.load(&candidate).unwrap().payload(), b"valid");
    let limits = CatalogLimits {
        max_issues: 1,
        ..CatalogLimits::default()
    };
    assert!(Catalog::open([&fixture.0], Limits::default(), limits).is_err());
}

#[test]
fn aggregate_bounds_and_unknown_metadata_extensions_fail_clearly() {
    let fixture = Fixture::new(&archive(&[(100, package(&[(42, b"valid", &[])]))], 1));
    for limits in [
        CatalogLimits {
            max_archives: 0,
            ..CatalogLimits::default()
        },
        CatalogLimits {
            max_entries: 0,
            ..CatalogLimits::default()
        },
        CatalogLimits {
            max_candidates: 0,
            ..CatalogLimits::default()
        },
        CatalogLimits {
            max_metadata_bytes: 15,
            ..CatalogLimits::default()
        },
    ] {
        assert!(Catalog::open([&fixture.0], Limits::default(), limits).is_err());
    }
    assert!(
        Catalog::open(
            Vec::<PathBuf>::new(),
            Limits::default(),
            CatalogLimits::default()
        )
        .is_err()
    );
    let mut metadata = 0u16.to_le_bytes().to_vec();
    metadata.push(99);
    let extension = Fixture::new(&archive(&[(100, raw_frame(&metadata))], 1));
    let catalog =
        Catalog::open([&extension.0], Limits::default(), CatalogLimits::default()).unwrap();
    assert!(!catalog.index_complete());
    assert_eq!(catalog.report().resource_candidates, 0);
    assert!(catalog.report().issues[0].error.contains("extension"));
}

#[test]
#[ignore = "requires ACU_GAME_DIR; selected base archive metadata only, then one requested package"]
fn installed_catalog_loads_original_default_texture() -> acu_assets::Result<()> {
    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: set ACU_GAME_DIR")?;
    let mut catalog = Catalog::open(
        [PathBuf::from(root).join("DataPC.forge")],
        Limits::default(),
        CatalogLimits::default(),
    )?;
    assert!(catalog.index_complete());
    assert_eq!(catalog.report().entries, 1620);
    assert_eq!(catalog.report().skipped_control_entries, 2);
    assert_eq!(catalog.report().indexed_packages, 1618);
    assert_eq!(catalog.report().resource_candidates, 45903);
    let candidate = catalog.unique("0000000000000008")?.clone();
    assert_eq!((candidate.entry_index, candidate.resource_index), (1, 0));
    let loaded = catalog.load(&candidate)?;
    assert_eq!(
        acu_assets::sha256(&loaded.package.bytes),
        "9fe2ca94dd611b45eb33a25d6c29ef104dfd3aeb29546f2649eb9a790e73b9a0"
    );
    let texture = loaded.package.texture(loaded.resource_index)?;
    assert_eq!((texture.width, texture.height), (4, 4));
    assert_eq!(texture.pixels, [80, 80, 80, 255].repeat(16));
    Ok(())
}
