//! Install-dependent Cloth resource checks (ACU-RE-0081).

use acu_assets::cloth::{self, Pointer};
use acu_assets::{Limits, Result, forge::Forge, package, sha256};
use std::path::Path;

/// `Softbody_CN_P_Flap_ArnaudV2`: prefix and LOD0 simulation blob decode;
/// particles lie in the flap mesh's space.
#[test]
#[ignore = "requires ACU_GAME_DIR; BelleEpoque_assets entry 5882"]
fn installed_tailored_flap_cloth_decodes() -> Result<()> {
    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: ACU_GAME_DIR is unset")?;
    let mut forge = Forge::open(
        Path::new(&root).join("DataPC_ACU_LGS_BelleEpoque_assets.forge"),
        Limits::default(),
    )?;
    let raw = forge.raw(5882)?;
    let p = package::decode(&raw, Limits::default())?;
    let r = &p.resources[0];
    assert_eq!(r.name, "Softbody_CN_P_Flap_ArnaudV2");
    assert_eq!(r.type_id, "e33044ba");
    let payload = &p.bytes[r.payload.clone()];
    assert_eq!(
        sha256(payload),
        "5fdcc55f709c1875f2b00b7194798bc75e118ae3a21c583701732d00502d4587"
    );
    let c = cloth::parse(payload, &r.file_id)?;
    assert_eq!(c.state_id, "0000001460e45331");
    assert_eq!(
        c.graphic_object,
        Pointer::Id {
            kind: 4,
            file_id: "00000013c1e4eb52".into()
        }
    );
    assert_eq!(c.lod_count, 4);
    assert_eq!(c.lod0_id, "0000001460e45332");
    assert_eq!(
        c.lod0_settings,
        Pointer::Id {
            kind: 0,
            file_id: "00000016c61d53a1".into()
        }
    );
    let b = &c.lod0_blob;
    assert_eq!(b.bytes.len(), 42195);
    assert_eq!(b.settings_name()?, Some("ClothSettings_Standard_Avatars"));
    assert_eq!(b.body_kind()?, 1);
    assert_eq!(b.particle_count()?, 587);
    let ps = b.particles()?;
    assert_eq!(ps[0][..3], [-0.341_827_57, 0.131_184_13, 0.359_484_67]);
    assert!(ps.iter().all(|p| p[3] == 0.0 || p[3] == -1.0));
    assert_eq!(c.lod0_rest_offset, 45813);
    Ok(())
}
