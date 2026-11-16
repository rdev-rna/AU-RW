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
    // LOD0 fields after the blob: particle skinning and the render-vertex mapping.
    let m = cloth::lod0_mesh(payload, &c)?;
    assert_eq!(m.mesh_id, "00000013c1e4eb9a");
    assert_eq!(m.triangles, b.triangles()?);
    assert_eq!(
        (m.mappings.len(), m.duplicates.len(), m.indices.len()),
        (1192, 203, 5385)
    );
    assert_eq!(m.mappings.iter().filter(|v| v.skinned).count(), 234);
    assert_eq!(
        (m.lod_index, m.particle_influences, m.vertex_influences),
        (0, 8, 8)
    );
    assert_eq!(m.bones.len(), 18);
    for (r, p) in m.rest_positions.iter().zip(&ps) {
        assert_eq!(r[..], p[..3]);
    }
    for w in m.particle_weights.chunks(8) {
        assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-4);
    }
    assert_eq!(m.end_offset, 180560 + 18 * 81);
    // Collision settings and the leg colliders (SoftBodyState.Ragdoll).
    let cs = st.collision;
    assert!(cs.rigid && cs.swept && cs.mesh && !cs.cloth);
    assert_eq!((cs.friction, cs.priority), (0.0, 128));
    let tail = &payload[payload.len() - 64..];
    let pointer = [&[1u8, 0][..], &0x0000_0027_d187_b440u64.to_le_bytes()].concat();
    assert_eq!(tail.windows(10).filter(|w| *w == pointer).count(), 1);
    let rr = &p.resources[2];
    assert_eq!(
        (rr.name.as_str(), rr.type_id.as_str()),
        ("LiteRagdoll_SkirtHV_Leg_ArnoBrigand", "891043d5")
    );
    let shapes = cloth::lite_ragdoll(&p.bytes[rr.payload.clone()], &rr.file_id)?;
    let bones: Vec<u32> = shapes.iter().map(|s| s.bone).collect();
    assert_eq!(
        bones,
        [
            0x3952_b745,
            0x9617_63c6,
            0x8856_5c0b,
            0xf160_2d3a,
            0xded1_0611,
            0x9b14_362c,
            0x5898_8870
        ]
    );
    assert!(shapes.iter().all(|s| s.shape_type == 0));
    assert_eq!((shapes[0].height, shapes[0].radius), (0.16, 0.12));
    let t4 = shapes[4].local[3];
    assert!((t4[0] - 0.000_117).abs() < 1e-5 && (t4[2] + 0.002_837).abs() < 1e-5);
    assert!((t4[3] - 1.0).abs() < 1e-6);
    // The external settings the LOD names: resource 1 of the same entry. Values equal the
    // live body's (ACU-RE-0081, live comparison 2026-11-16).
    let sr = &p.resources[1];
    assert_eq!(
        (sr.name.as_str(), sr.type_id.as_str(), sr.file_id.as_str()),
        (
            "ClothSettings_Standard_Avatars",
            "f7a7e4df",
            "00000016c61d53a1"
        )
    );
    let sp = &p.bytes[sr.payload.clone()];
    assert_eq!(
        sha256(sp),
        "13efc992ab82da050e34ec64fc87f6a306f543347c77fb98f5bd8aedb9b42764"
    );
    let s = cloth::soft_body_settings(sp, &sr.file_id)?;
    assert_eq!(s.gravity(), [0.0, 0.0, -20.0]);
    assert_eq!(
        (s.damping(), s.thickness(), s.tether_scale()),
        (0.0, 0.05, 1.1)
    );
    assert_eq!((s.friction(), s.drag()), (0.2, (30.0, 15.0)));
    // Opcode enables (gate 0x14240bf70) as the blob writes them: all on, drag (S+64) on. The
    // live flap had blend (S+6) and drag off because external settings apply (ACU-RE-0081).
    let g = b.op_gate()?;
    assert_eq!(g.s0_6, Some([true; 7]));
    assert_eq!(
        (g.s7, g.s8, g.s9, g.s10, g.s64),
        (true, true, false, true, true)
    );
    // Per-set (stiffness, k_min, k_max): the blob's clamps, live stiffness 1, 1, 1.5.
    for (k, set) in sets.iter().enumerate() {
        let (stiffness, k_min, k_max) = s.distance(k);
        assert_eq!((k_min, k_max), (set.k_min, set.k_max));
        assert_eq!(stiffness, [1.0, 1.0, 1.5][k]);
    }
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
