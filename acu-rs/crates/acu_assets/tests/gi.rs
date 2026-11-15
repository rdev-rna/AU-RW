//! Install-dependent GI / ambiance checks (ACU-RE-0073). Missing data is reported as
//! unavailable, never as success.

use acu_assets::{Limits, Result, ambiance, forge::Forge, gi, gicp, package, sha256};
use std::path::{Path, PathBuf};

fn game_dir() -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: set ACU_GAME_DIR")?,
    ))
}

/// Payload of resource `file_id` in `archive` entry `entry`.
fn payload(archive: &Path, entry: usize, file_id: &str) -> Result<Vec<u8>> {
    let mut forge = Forge::open(archive, Limits::default())?;
    let p = package::decode(&forge.raw(entry)?, Limits::default())?;
    let r = p
        .resources
        .iter()
        .find(|r| r.file_id == file_id)
        .ok_or_else(|| format!("{file_id} not in entry {entry}"))?;
    Ok(p.bytes[r.payload.clone()].to_vec())
}

/// The four Paris GILoaders (entry 21267) decode to their exact end; each names its
/// time-of-day Season and links GIDatablocks.
#[test]
#[ignore = "requires ACU_GAME_DIR; DataPC_ACU_Paris.forge entry 21267"]
fn installed_paris_gi_loaders_decode() -> Result<()> {
    let archive = game_dir()?.join("DataPC_ACU_Paris.forge");
    for (fid, hash, season, nodes, leaves) in [
        (
            "0000003a66d8e3b4",
            "05265fa6130201cc2b65c789f1211e3d33d52b8f48b86ec75dd8a04438755c31",
            0x7_f5f1_bc03u64, // PAR_TOD_Paris_Night
            61_000,
            61_000,
        ),
        (
            "0000003a66d8e3b5",
            "89015f4e0c0c86a957f4b2c070ed28040fbbc444187da97a89f0dd52baf507a5",
            0x7_f5f1_bb41, // PAR_TOD_Paris_Dawn
            65_518,
            65_454,
        ),
        (
            "0000003a66d8e3b6",
            "7dc51e496ee15afd00ffd3890a233551a828d748c8ce11adee8d8fce0c68c738",
            0x7_f5f1_bc0b, // PAR_TOD_Paris_Noon
            65_028,
            65_020,
        ),
        (
            "0000003a66d8e3b7",
            "651ec84491de3ec09e346ba717e1c03e9283424a436d6a95bd7490ef0e14bce8",
            0x7_f5f1_bc07, // PAR_TOD_Paris_Sunset
            62_691,
            62_691,
        ),
    ] {
        let bytes = payload(&archive, 21267, fid)?;
        assert_eq!(sha256(&bytes), hash, "{fid}");
        let l = gi::decode_loader(&bytes)?;
        assert_eq!(l.season, season, "{fid}");
        assert_eq!(l.node_count, nodes, "{fid}");
        assert_eq!(l.leaves().len(), leaves, "{fid}");
        assert_eq!(l.vector80, [32.0; 3]);
        assert_eq!(l.vector92, [16.0; 3]);
        assert_eq!(l.vector16, [-960.0, -864.0, -112.0, 0.0]);
        assert_eq!(l.vector32, [1568.0, 1376.0, 128.0, 0.0]);
    }
    Ok(())
}

/// A Noon GIDatablock from the `_gi` archive decodes to its exact end: 12 cells of
/// 16 m, each with four GIData of four GICP blobs.
#[test]
#[ignore = "requires ACU_GAME_DIR; DataPC_ACU_Paris_gi.forge entry 3338"]
fn installed_paris_gi_datablock_decodes() -> Result<()> {
    let archive = game_dir()?.join("DataPC_ACU_Paris_gi.forge");
    let bytes = payload(&archive, 3338, "0000001758716bd6")?;
    assert_eq!(bytes.len(), 743_650);
    assert_eq!(
        sha256(&bytes),
        "7e0c8a3d8fbad11f5ca8f824330351cf30643e48ac9526c030216a441ad08c0b"
    );
    let d = gi::decode_datablock(&bytes)?;
    assert_eq!(d.season, 0x7_f5f1_bc0b);
    assert_eq!(d.results.len(), 12);
    let first = &d.results[0];
    assert_eq!(first.object_id, 0x18_62d4_bc09);
    assert_eq!(first.center, [-720.0, -336.0, -48.0, 0.0]);
    assert_eq!(first.extents, [16.0, 16.0, 16.0, 0.0]);
    for r in &d.results {
        assert_eq!(r.data.len(), 4);
        for g in &r.data {
            assert_eq!(g.blob_dims[0], [16, 16, 16]);
            assert_eq!(g.value100, 32);
        }
    }
    // The Noon loader links this datablock.
    let loader = gi::decode_loader(&payload(
        &game_dir()?.join("DataPC_ACU_Paris.forge"),
        21267,
        "0000003a66d8e3b6",
    )?)?;
    let linked = loader
        .leaves()
        .iter()
        .filter(|n| n.datablock.id == d.object_id)
        .count();
    assert!(linked > 0);
    Ok(())
}

/// The four Paris WorldAmbiance presets: SSAO / HDR prefix and the fixed shadow tail.
#[test]
#[ignore = "requires ACU_GAME_DIR; DataPC_ACU_Paris.forge entries 22301-22304"]
fn installed_paris_world_ambiance_decodes() -> Result<()> {
    let archive = game_dir()?.join("DataPC_ACU_Paris.forge");
    for (entry, fid, hash, value132) in [
        (
            22301,
            "00000007f5f1bb7b",
            "b044dd36028f0deaa49ca8a787451d06449c4def346de110162cfeb7e83e8c38",
            1.5f32,
        ),
        (
            22302,
            "00000007f5f1bb9d",
            "ff675df465dd9ffff6b9d66065ffe266f4355b71edacf4f2456f2f6fb5fe764d",
            -4.8,
        ),
        (
            22303,
            "00000007f5f1bbbf",
            "dca7ebd39ad8bef1a81883c248d4ed54770e24ca5d9e3181d4744a86d7b3572b",
            1.5,
        ),
        (
            22304,
            "00000007f5f1bbe1",
            "0814f1c888393c3bdd5fb3a2eae453f0ac8bc16a97a5689a46b7992e708ceca2",
            3.0,
        ),
    ] {
        let bytes = payload(&archive, entry, fid)?;
        assert_eq!(bytes.len(), 4037);
        assert_eq!(sha256(&bytes), hash, "{fid}");
        let w = ambiance::decode(&bytes)?;
        assert_eq!(w.forward_end, 407, "{fid}");
        assert_eq!(
            (w.hdr.aperture, w.hdr.shutter_reciprocal, w.hdr.iso),
            (5.6, 250.0, 100.0)
        );
        assert_eq!(w.hdr.value132, value132, "{fid}");
        assert_eq!(w.ssao.enabled, 1);
        assert_eq!(w.shadow.value3356, 1024);
        assert_eq!(w.shadow.pssm_shadow_end_distance, 128.0);
        assert_eq!(w.shadow.pssm_split_weight, 0.9);
        assert_eq!(w.shadow.pssm_shadow_noise_size, 3.0);
        assert_eq!(w.shadow.cascades3396, [5.0, 14.0, 38.0, 72.0]);
    }
    Ok(())
}

/// Every GICP blob of the Noon datablock decodes to its exact end (YCbCr path: one channel per
/// blob, `GIData +106` = 1). Expected values come from the independent Python prototype
/// (`local/lighting/gicp.py`, `gicp_ref.py`).
#[test]
#[ignore = "requires ACU_GAME_DIR; DataPC_ACU_Paris_gi.forge entry 3338"]
fn installed_paris_gicp_blobs_decode() -> Result<()> {
    let archive = game_dir()?.join("DataPC_ACU_Paris_gi.forge");
    let d = gi::decode_datablock(&payload(&archive, 3338, "0000001758716bd6")?)?;
    let mut blobs = 0;
    for r in &d.results {
        for g in &r.data {
            assert_eq!(g.byte106, 1);
            for b in &g.blobs {
                gicp::decode(b, 1)?;
                blobs += 1;
            }
        }
    }
    assert_eq!(blobs, 192);
    let first = &d.results[0];
    let vols: [[gicp::Volume; 4]; 4] = std::array::from_fn(|k| {
        std::array::from_fn(|j| gicp::decode(&first.data[k].blobs[j], 1).expect("decoded above"))
    });
    assert_eq!(
        sha256(&vols[0][0].values),
        "47126639316426e11fce59684fac7099ebab6b08e8b467ab2630c6c15a8f4ce2"
    );
    assert_eq!(vols[0][3].dim, 32);
    assert_eq!(
        sha256(&vols[0][3].values),
        "3f524085e00c41e41eff53cf760ddd2b6c49678bc52ad19ecbb64186059ef1c3"
    );
    let scale: [[f32; 4]; 4] = std::array::from_fn(|k| first.data[k].vector0);
    let bias: [[f32; 4]; 4] = std::array::from_fn(|k| first.data[k].vector16);
    let t = gicp::combine_ycbcr(&vols, &scale, &bias, [12, 0, 15], 16);
    let expect_rgb = [
        [0.021582, -0.033562, -0.003078, 0.053139],
        [0.015557, -0.024171, -0.002178, 0.038788],
        [0.008905, -0.013619, -0.001069, 0.021989],
    ];
    for (c, (got, want)) in t.rgb.iter().zip(&expect_rgb).enumerate() {
        for (j, (g, w)) in got.iter().zip(want).enumerate() {
            assert!((g - w).abs() < 2e-6, "{c} {j}: {g}");
        }
    }
    let expect_extra = [-0.014389, -0.075172, 0.098561, 0.333344];
    for (j, (g, w)) in t.extra.iter().zip(&expect_extra).enumerate() {
        assert!((g - w).abs() < 2e-6, "extra {j}: {g}");
    }
    Ok(())
}
