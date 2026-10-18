use acu_assets::{Limits, Result, forge::Forge, frame, prefetch, reader::Reader, sha256};

fn index(rows: &[(u64, u32, u32)], blob: &[u8]) -> Vec<u8> {
    let mut bytes = (rows.len() as u32).to_le_bytes().to_vec();
    for &(id, size, offset) in rows {
        bytes.extend(id.to_le_bytes());
        bytes.extend(size.to_le_bytes());
        bytes.extend(offset.to_le_bytes());
    }
    bytes.extend((blob.len() as u32).to_le_bytes());
    bytes.extend(blob);
    bytes
}

#[test]
fn shared_blocks_filters_and_dependencies() -> Result<()> {
    let mut block = vec![1, 0, 1, 128, 7]; // one flagged filter, one action
    block.extend(0xfedcba9876543210u64.to_le_bytes());
    block.extend([1, 0]); // one dependency
    block.extend(0x8000000000000001u64.to_le_bytes());
    block.extend([2, 1, 0, 0, 0]); // policy 2, one filter index, index 0
    let bytes = index(
        &[(9, block.len() as u32, 0), (9, block.len() as u32, 0)],
        &block,
    );
    let parsed = prefetch::parse(&bytes, Limits::default())?;
    assert_eq!(parsed.blocks.len(), 1);
    assert_eq!(parsed.entries.len(), 2); // preserve duplicate IDs; no precedence inferred
    assert_eq!(parsed.entries[0].block_index, parsed.entries[1].block_index);
    assert!(parsed.blocks[0].filters[0].flag);
    assert_eq!(parsed.blocks[0].filters[0].actions[0].action, 7);
    assert_eq!(
        parsed.blocks[0].filters[0].actions[0].data_layer_id,
        "fedcba9876543210"
    );
    assert_eq!(parsed.blocks[0].dependencies[0].file_id, "8000000000000001");
    assert_eq!(parsed.blocks[0].dependencies[0].filter_indices, [0]);
    let mut bad = block.clone();
    let last = bad.len() - 2;
    bad[last] = 1;
    assert!(prefetch::parse(&index(&[(9, bad.len() as u32, 0)], &bad), Limits::default()).is_err());
    Ok(())
}

#[test]
fn malformed_ranges_and_nested_consumption() -> Result<()> {
    let empty = [0u8; 4];
    let mut frame = 0x1004fa9957fbaa33u64.to_le_bytes().to_vec();
    frame.extend([1, 0, 0, 0, 0, 0, 0, 0]); // version, algorithm, flags, raw marker
    frame.extend(4u32.to_le_bytes());
    frame.extend(empty);
    let valid = index(&[(9, 0x80000000 | frame.len() as u32, 0)], &frame);
    assert_eq!(prefetch::parse(&valid, Limits::default())?.blocks.len(), 1);
    for length in 0..valid.len() {
        assert!(prefetch::parse(&valid[..length], Limits::default()).is_err());
    }
    for rows in [
        vec![(9, 4, 1)],
        vec![(9, 3, 0)],
        vec![(9, 4, 0), (10, 3, 1)],
        vec![(9, 0, 0)],
        vec![(9, 4, 0), (10, 0x80000004, 0)],
    ] {
        assert!(prefetch::parse(&index(&rows, &empty), Limits::default()).is_err());
    }
    let mut trailing = frame;
    trailing.push(99);
    assert!(
        prefetch::parse(
            &index(&[(9, 0x80000000 | trailing.len() as u32, 0)], &trailing),
            Limits::default()
        )
        .is_err()
    );
    assert!(
        prefetch::parse(
            &valid,
            Limits {
                max_decoded: valid.len() - 1,
                ..Limits::default()
            }
        )
        .is_err()
    );
    Ok(())
}

#[test]
#[ignore = "requires ACU_GAME_DIR; all installed control0x91 compared with independent native LZO census"]
fn installed_prefetch_native_oracle_census() -> Result<()> {
    let root = std::path::PathBuf::from(
        std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: set ACU_GAME_DIR")?,
    );
    let archives = acu_assets::forge::discover(&root)?;
    let (mut controls, mut entries, mut blocks, mut deps, mut filters) = (0, 0, 0, 0, 0);
    for path in archives {
        let mut archive = Forge::open(&path, Limits::default())?;
        let indices: Vec<_> = archive
            .entries
            .iter()
            .filter(|e| e.file_id == "0000000000000091")
            .map(|e| e.index)
            .collect();
        for index in indices {
            let raw = archive.raw(index)?;
            let mut r = Reader::new(&raw);
            let (decoded, _) = frame::decode(&mut r, Limits::default())?;
            assert_eq!(r.remaining(), 0);
            if path.file_name().unwrap() == "DataPC.forge" {
                assert_eq!(
                    sha256(&raw),
                    "409ae7104a83cca0bf68bd7a46b2b0785221d7e6d3befef45a54fe8f56444db0"
                );
                assert_eq!(
                    sha256(&decoded),
                    "3b2c78136bb91a5161d4e31df11f5d629f42737a032be9539297ce5a54b1d8c9"
                );
            }
            let parsed = prefetch::parse(&decoded, Limits::default())?;
            controls += 1;
            entries += parsed.entries.len();
            blocks += parsed.blocks.len();
            deps += parsed
                .blocks
                .iter()
                .map(|b| b.dependencies.len())
                .sum::<usize>();
            filters += parsed.blocks.iter().map(|b| b.filters.len()).sum::<usize>();
        }
    }
    assert_eq!(
        (controls, entries, blocks, deps, filters),
        (63, 180119, 30679, 2344758, 19816)
    );
    Ok(())
}
