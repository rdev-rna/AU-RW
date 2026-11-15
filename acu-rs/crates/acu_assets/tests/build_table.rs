//! Install-dependent BuildTable checks (ACU-RE-0016, ACU-RE-0071).

use acu_assets::build_table::{self, Value};
use acu_assets::{Limits, Result, forge::Forge, package, sha256};
use std::path::Path;

fn table(index: usize) -> Result<(Vec<u8>, String, String)> {
    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: ACU_GAME_DIR is unset")?;
    let mut forge = Forge::open(
        Path::new(&root).join("DataPC_ACU_LGS_BelleEpoque.forge"),
        Limits::default(),
    )?;
    let raw = forge.raw(334)?;
    let p = package::decode(&raw, Limits::default())?;
    let r = &p.resources[index];
    let payload = p.bytes[r.payload.clone()].to_vec();
    Ok((payload, r.file_id.clone(), r.name.clone()))
}

/// Gear tables decode to their exact end with the ACU-RE-0016 payload hashes.
#[test]
#[ignore = "requires ACU_GAME_DIR; BelleEpoque entry 334 tables"]
fn installed_gear_tables_decode() -> Result<()> {
    for (index, hash, rows) in [
        (
            122,
            "dcf14a345863b643478ab1aaabeedef1837a1e72c5ddf2809a98298fa447835b",
            43,
        ),
        (
            123,
            "bb80cefe1d9af92b9fdac74aa5b12964f999be72521496f89bcb3f3fa96a14ee",
            43,
        ),
        (
            124,
            "5c40b98de04faeae56d9c9c5129739cfef548049eedd48e4f16d258e843af7aa",
            43,
        ),
        (
            125,
            "337dafc2679505be53401fcda04f7f8752f933fcc2ad8437db18efb299fd67db",
            43,
        ),
    ] {
        let (payload, id, _) = table(index)?;
        assert_eq!(sha256(&payload), hash);
        let t = build_table::parse(&payload, &id)?;
        assert_eq!(t.rows.len(), rows);
        assert_eq!(t.rows.iter().filter(|r| r.weight == 1.0).count(), 1);
    }
    Ok(())
}

/// `CN_P_Avatars_ColorSets`: 8 material-path columns, 28 rows; row 1 is the
/// only weighted row. Values are independent of the Python diagnostic.
#[test]
#[ignore = "requires ACU_GAME_DIR; BelleEpoque entry 334 resource 127"]
fn installed_color_set_table_decodes() -> Result<()> {
    let (payload, id, name) = table(127)?;
    assert_eq!(name, "CN_P_Avatars_ColorSets");
    assert_eq!(id, "0000001e7c52e192");
    assert_eq!(payload.len(), 11013);
    let t = build_table::parse(&payload, &id)?;
    assert_eq!(t.columns.len(), 8);
    assert_eq!(t.rows.len(), 28);
    let weighted: Vec<_> = t
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.weight == 1.0)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(weighted, [1]);
    let targets: Vec<_> = t
        .columns
        .iter()
        .map(|c| {
            c.path
                .iter()
                .map(|n| n.target_property.as_str())
                .filter(|t| *t != "00000000")
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    assert_eq!(
        targets,
        [
            "b000af3e",
            "dddd4bd5",
            "adb7bf5a",
            "25ec2112/b000af3e",
            "25ec2112/dddd4bd5",
            "25ec2112/adb7bf5a",
            "84f72d37",
            "34beeee0"
        ]
    );
    for c in &t.columns {
        let s = c.path[0]
            .solver
            .as_ref()
            .expect("first node is a material solver");
        assert_eq!((s.mode, s.match_mask[9]), (1, 1));
        assert!(
            s.match_mask
                .iter()
                .enumerate()
                .all(|(i, b)| i == 9 || *b == 0)
        );
    }
    let row = &t.rows[1];
    assert_eq!(row.tag, 0x84f0_c283);
    let get = |col: i32| {
        row.components
            .iter()
            .find(|(k, _)| *k == col)
            .map(|(_, p)| p.value.clone())
    };
    assert_eq!(get(2), Some(Value::Vector4([0.62, 1.2, 1.0, 0.0])));
    assert_eq!(get(1), Some(Value::Vector4([1.02, 0.5, 1.0, 0.0])));
    assert_eq!(get(9), Some(Value::Float(0.05)));
    Ok(())
}
