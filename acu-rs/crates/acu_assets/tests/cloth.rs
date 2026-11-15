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
    // Distance sets 0-2 (structural/shear/bend): counts, and quantised rest
    // lengths within half a step of the rest-pose particle distances.
    let sets = b.distance_sets()?;
    let n: Vec<_> = sets.iter().map(|s| (s.pairs.len(), s.simd_count)).collect();
    assert_eq!(n, [(1132, 1104), (988, 976), (1212, 1192)]);
    let clamp: Vec<_> = sets.iter().map(|s| (s.k_min, s.k_max)).collect();
    assert_eq!(clamp, [(-2.0, 2.0), (0.0, 2.0), (-2.0, 0.0)]);
    for s in &sets {
        assert_eq!(s.stiffness, 1.0);
        for (j, &(a, bb)) in s.pairs.iter().enumerate() {
            let (p, q) = (ps[a as usize], ps[bb as usize]);
            let len =
                ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2)).sqrt();
            let rest = s.rest_q[j] as f32 * s.scale + s.offset;
            assert!((rest - len).abs() <= s.scale * 0.5 + 1e-6, "pair {j}");
            assert!(rest < s.threshold);
        }
    }
    // Sets 3-5, maps, flags, settings, triangles and program (ACU-RE-0081).
    let pc = b.particle_constraints()?;
    assert_eq!((pc.backstop.bytes.len(), pc.anchors.len()), (592, 592));
    let pinned: Vec<usize> = (0..587).filter(|&i| ps[i][3] < 0.0).collect();
    assert_eq!(pinned.len(), 180);
    assert!(pc.anchors[..587].iter().all(|&a| ps[a as usize][3] < 0.0));
    let f = b.flags()?;
    assert_eq!((f[7], f[22], f[28], f[29], f[30]), (1, 0, 1, 0, 1));
    let maps = b.particle_maps()?;
    assert!(maps[0].is_some() && maps[2].is_some() && maps[1].is_none());
    let st = b.settings()?;
    assert_eq!(st.gravity, [0.0, 0.0, -10.0]);
    assert_eq!((st.damping, st.max_speed, st.blend), (0.0, 1000.0, 0.0));
    assert_eq!((st.drag.a, st.drag.b), (1.3, 0.7));
    assert_eq!(b.triangles()?.len(), 1039);
    assert_eq!(
        b.schedule()?,
        [
            (6, 1),
            (9, 1),
            (5, 1),
            (10, 4),
            (2, 1),
            (1, 1),
            (0, 1),
            (7, 1),
            (3, 1),
            (4, 1),
            (13, 1),
            (11, 1),
            (8, 1)
        ]
    );
    Ok(())
}
