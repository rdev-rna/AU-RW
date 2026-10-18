//! Explicit install-dependent checks; absent inputs are unavailable, never parity success.
use acu_assets::{Limits, Result, forge::Forge, package, sha256};
use std::path::PathBuf;

#[test]
#[ignore = "requires ACU_GAME_DIR; independent native LZO oracle hashes in tools/validate_lzo.py report"]
fn runtime_base_and_patch_vectors() -> Result<()> {
    let root =
        PathBuf::from(std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: set ACU_GAME_DIR")?);
    for (name, index, stored, decoded, resource_count) in [
        (
            "DataPC.forge",
            1,
            "9d647e486d3f2287821d5173d8da2190929d6bec7908c447b51a7ccd8679ac5b",
            "9fe2ca94dd611b45eb33a25d6c29ef104dfd3aeb29546f2649eb9a790e73b9a0",
            1,
        ),
        (
            "DataPC_extra_patch_01.forge",
            0,
            "7a624195a8399b8e88e02a741f6ac5867849444c3ca976063f4a24b4dc877ab9",
            "d58914b7f91d2b9ab99075d4efd1116180e1599317c0fd5529bbe0ad49824b52",
            2,
        ),
    ] {
        let mut forge = Forge::open(root.join(name), Limits::default())?;
        let raw = forge.raw(index)?;
        assert_eq!(sha256(&raw), stored, "stored fixture identity changed");
        let package = package::decode(&raw, Limits::default())?;
        assert_eq!(
            sha256(&package.bytes),
            decoded,
            "differs from independent native LZO oracle"
        );
        assert_eq!(package.resources.len(), resource_count);
        if name == "DataPC.forge" {
            let resource = &package.resources[0];
            let texture = acu_assets::texture::parse(
                &package.bytes[resource.payload.clone()],
                &resource.file_id,
            )?;
            assert_eq!(
                (texture.width, texture.height, texture.mip_count),
                (4, 4, 1)
            );
            assert_eq!(texture.dxgi_format, 87);
            assert_eq!(texture.pixels, [80, 80, 80, 255].repeat(16));
        }
    }
    let mut archive = Forge::open(
        root.join("DataPC_ACU_LGS_BelleEpoque.forge"),
        Limits::default(),
    )?;
    let raw = archive.raw(38)?;
    assert_eq!(
        sha256(&raw),
        "92e337e803c4169ed2a0d807bcb48d88f5b678d6e903c1b634344d7b85fba11f"
    );
    let package = package::decode(&raw, Limits::default())?;
    assert_eq!(
        sha256(&package.bytes),
        "60e30c88d97db214d1e08796038004002216d0837fbae2b564dff5a98f1dd702"
    );
    assert_eq!(package.resources.len(), 375);
    assert_eq!(package.resources[35].file_id, "0000001dcfeacc01");
    assert_eq!(package.resources[35].metadata_tags, vec![0]);
    assert!(package.metadata_extension.is_empty());
    Ok(())
}

#[test]
#[ignore = "requires ACU_GAME_DIR; metadata byte identities/independent Python parse in ACU-RE-0004"]
fn original_global_metadata() -> Result<()> {
    use acu_assets::control::MetaValue;
    let root =
        PathBuf::from(std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: set ACU_GAME_DIR")?);
    let mut archive = Forge::open(root.join("DataPC.forge"), Limits::default())?;
    assert_eq!(
        sha256(&archive.raw(0)?),
        "30fff3eef19744b637187964d8ad253884c39c62d6233ec63d43953a3fb7f7c3"
    );
    let meta = archive.read_global_metadata(0)?;
    assert_eq!(meta.version, 1);
    assert_eq!(
        meta.attributes.iter().find(|a| a.key == 2).unwrap().value,
        MetaValue::U32(335689)
    );
    assert_eq!(
        meta.attributes.iter().find(|a| a.key == 12).unwrap().value,
        MetaValue::U32(1059514)
    );
    assert!(archive.read_global_metadata(1).is_err());
    Ok(())
}
