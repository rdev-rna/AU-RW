use acu_assets::{Limits, Result, forge::Forge, package, sha256, skeleton};

#[test]
#[ignore = "requires original ACU installation; ACU_GAME_DIR"]
fn original_arno_head_rig() -> Result<()> {
    let root = std::env::var("ACU_GAME_DIR")?;
    let mut forge = Forge::open(
        std::path::Path::new(&root).join("DataPC_ACU_LGS_BelleEpoque.forge"),
        Limits::default(),
    )?;
    let raw = forge.raw(334)?;
    assert_eq!(
        sha256(&raw),
        "42935733c7319be475e358525ef99582a31dbf9b375825c8802244b9d4229137"
    );
    let package = package::decode(&raw, Limits::default())?;
    let resource = &package.resources[66];
    assert_eq!(resource.file_id, "00000012ff1479d5");
    let payload = &package.bytes[resource.payload.clone()];
    assert_eq!(payload.len(), 97123);
    assert_eq!(
        sha256(payload),
        "1c3f7c7919d641f7fcf0fdfcd5814a28b53f17a03446dbbaafc7548e1dcbe341"
    );
    let rig = skeleton::parse(payload, &resource.file_id, Limits::default())?;
    // Independent Python/native-LZO sequential decode, ACU-RE-0016.
    assert_eq!(rig.bones.len(), 134);
    assert_eq!(rig.pose_groups.len(), 1);
    assert_eq!(rig.pose_groups[0].poses.len(), 147);
    let pose_bones: Vec<_> = rig.pose_groups[0]
        .poses
        .iter()
        .flat_map(|p| &p.bones)
        .collect();
    assert_eq!(pose_bones.len(), 2526);
    assert_eq!(pose_bones[0].bone_id, 114683999);
    assert_eq!(pose_bones[0].packed_position, [1456, -8230, -4409]);
    assert_eq!(pose_bones[0].packed_rotation, [-16401, -15403, 16756]);
    let modifiers: Vec<_> = rig.bones.iter().flat_map(|b| &b.modifiers).collect();
    assert_eq!(modifiers.len(), 106);
    assert!(
        modifiers
            .iter()
            .all(|m| matches!(m.data, skeleton::humanoid::ModifierData::Wrinkle { .. }))
    );
    Ok(())
}

fn fixture(parents: &[Option<u64>]) -> Vec<u8> {
    let mut b = vec![1];
    b.extend(10u64.to_le_bytes());
    b.extend(skeleton::CLASS_SKELETON.to_le_bytes());
    b.extend(8u32.to_le_bytes());
    b.extend((parents.len() as u32).to_le_bytes());
    for (i, parent) in parents.iter().enumerate() {
        b.push(0);
        b.extend((100 + i as u64).to_le_bytes());
        b.extend(skeleton::CLASS_BONE.to_le_bytes());
        b.extend((200 + i as u32).to_le_bytes());
        match parent {
            Some(id) => {
                b.push(2);
                b.extend(id.to_le_bytes());
            }
            None => b.push(3),
        }
        b.push(3); // mirror null
        for _ in 0..4 {
            for f in [0f32, 0., 0., 1.] {
                b.extend(f.to_le_bytes());
            }
        }
        b.push(0);
        b.extend(5u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(7u32.to_le_bytes());
        b.extend(1f32.to_le_bytes());
        b.extend((i as u16).to_le_bytes());
        b.extend(0u16.to_le_bytes());
    }
    b.push(2);
    b.extend(100u64.to_le_bytes());
    for _ in 0..2 {
        for f in [0f32, 0., 0., 1.] {
            b.extend(f.to_le_bytes());
        }
    }
    b.extend([3, 0]);
    b.extend(0u64.to_le_bytes()); // body reference
    b.extend(0u32.to_le_bytes()); // posegroups
    b.extend([3, 3]); // IK/COM
    b.extend([3, 0]);
    b.extend(0u64.to_le_bytes()); // lookat reference
    b.push(3); // reflex
    b.extend(0x1234u32.to_le_bytes());
    b.extend(0x5678u32.to_le_bytes());
    b
}

#[test]
fn native_values_and_graph_validation() -> Result<()> {
    let b = fixture(&[None, Some(100)]);
    let parsed = skeleton::parse(&b, "000000000000000a", Limits::default())?;
    assert_eq!(parsed.bones.len(), 2);
    assert_eq!(parsed.bones[1].name_hash, 201);
    assert_eq!(parsed.bones[1].local_rotation, [0., 0., 0., 1.]);
    assert_eq!(parsed.hierarchy_key, 0x5678);
    assert!(
        skeleton::parse(
            &fixture(&[Some(100)]),
            "000000000000000a",
            Limits::default()
        )
        .is_err()
    );
    assert!(
        skeleton::parse(
            &fixture(&[Some(101), Some(100)]),
            "000000000000000a",
            Limits::default()
        )
        .is_err()
    );
    // Additive parent outside this resource remains an explicit unresolved link.
    skeleton::parse(
        &fixture(&[Some(999)]),
        "000000000000000a",
        Limits::default(),
    )?;
    assert!(skeleton::parse(&b, "000000000000000b", Limits::default()).is_err());
    Ok(())
}

#[test]
fn truncation_unsupported_and_bounds() {
    let b = fixture(&[None]);
    for n in 0..b.len() {
        assert!(skeleton::parse(&b[..n], "000000000000000a", Limits::default()).is_err());
    }
    let mut bad = b.clone();
    bad.push(0);
    assert!(skeleton::parse(&bad, "000000000000000a", Limits::default()).is_err());
    let mut bad = b.clone();
    bad[40..44].copy_from_slice(&f32::NAN.to_le_bytes()); // first transform
    assert!(skeleton::parse(&bad, "000000000000000a", Limits::default()).is_err());
    let mut bad = b.clone();
    bad[109..113].copy_from_slice(&1u32.to_le_bytes()); // modifier count
    assert!(skeleton::parse(&bad, "000000000000000a", Limits::default()).is_err());
    assert!(
        skeleton::parse(
            &b,
            "000000000000000a",
            Limits {
                max_resources: 0,
                ..Limits::default()
            }
        )
        .is_err()
    );
}

#[test]
#[ignore = "requires ACU_GAME_DIR; native LZO package and payload identities in ACU-RE-0010"]
fn original_skeleton_subset() -> Result<()> {
    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: set ACU_GAME_DIR")?;
    let mut forge = Forge::open(
        std::path::Path::new(&root).join("DataPC.forge"),
        Limits::default(),
    )?;
    let raw = forge.raw(110)?;
    assert_eq!(
        sha256(&raw),
        "7d5f500049eeb22158005bbd74fb8c22b84b84501d226aa6bfc66c4b0aa2334f"
    );
    let package = package::decode(&raw, Limits::default())?;
    assert_eq!(
        sha256(&package.bytes),
        "dc857aaed9c39456ef246fab5231a512583f9143acd587469b73a0e4c9242400"
    );
    let mut parsed = 0;
    let mut unsupported = 0;
    // Independent sequential Python/native-LZO probe, ACU-RE-0014.
    let supported_names = [
        "W_WeaponSimple_Root",
        "Weapon_Collapsible",
        "ACU_BaseSkeleton_Male",
        "ACU_Base",
        "Generic_Head_Male_SKELETON",
        "GFX_Bombs_Ballistic_Curve",
        "CHR_W_ChainBlade",
        "Addon_Human_Weapon_Ref",
        "Addon_Human_Crossbow",
        "Addon_Human_Mousquet",
        "Addon_Human_ChainBlade",
        "Addon_Connor_Bow",
        "Addon_Connor_HiddenBlade",
        "Addon_Connor_HiddenKnife",
        "ACU_Unisex_FrontShirtBend",
        "CN_P_Arno_HiddenBlade_Left_A",
        "ACU_Unisex_ArmsSimple",
        "ACU_Unisex_LCol",
        "ACU_Unisex_LegsSimple",
        "GFX_RigidBodies_Rock",
    ];
    for resource in package
        .resources
        .iter()
        .filter(|r| r.type_id == skeleton::TYPE_SKELETON)
    {
        let bytes = &package.bytes[resource.payload.clone()];
        match skeleton::parse(bytes, &resource.file_id, Limits::default()) {
            Ok(skeleton) => {
                assert!(
                    supported_names.contains(&resource.name.as_str()),
                    "unexpected success: {}",
                    resource.name
                );
                parsed += 1;
                if resource.name == "W_WeaponSimple_Root" {
                    assert_eq!(
                        sha256(bytes),
                        "60538e90ad851840f7dbdc6647d7a8b4ac2128215d28e7d971ceff1789e15864"
                    );
                    assert_eq!(skeleton.bones[0].file_id, "00000015bcf63f5d");
                    assert_eq!(skeleton.bones[0].name_hash, 3066451557);
                    assert_eq!(
                        (skeleton.skeleton_key, skeleton.hierarchy_key),
                        (224028746, 224028746)
                    );
                }
                if resource.name == "Weapon_Collapsible" {
                    assert_eq!(
                        sha256(bytes),
                        "6bd78565ba21d6594ddbce64df6c4de860e0e23e90a9cd8f87b35487be7ee5c3"
                    );
                    assert_eq!(skeleton.bones.len(), 2);
                    assert_eq!(
                        (skeleton.bones[0].children_count, skeleton.bones[1].index),
                        (1, 1)
                    );
                }
                if resource.name == "ACU_BaseSkeleton_Male" {
                    assert_eq!(
                        sha256(bytes),
                        "28cf74f0bc1ae0ad05504f4c91350914a28c30eb69ee7a51b73f4ca1e8da4ba1"
                    );
                    assert_eq!(skeleton.bones.len(), 109);
                    let mut classes = std::collections::BTreeMap::new();
                    for modifier in skeleton.bones.iter().flat_map(|b| &b.modifiers) {
                        *classes
                            .entry(modifier.class_hash.as_str())
                            .or_insert(0usize) += 1;
                    }
                    assert_eq!(
                        classes,
                        std::collections::BTreeMap::from([
                            ("75116750", 32),
                            ("49f4ca3e", 6),
                            ("d0c34a81", 4),
                            ("057daa86", 2)
                        ])
                    );
                    let skeleton::Pointer::Inline {
                        object, managed, ..
                    } = &skeleton.reflex_system
                    else {
                        panic!("missing male Reflex system")
                    };
                    assert!(managed.is_some());
                    let skeleton::humanoid::ObjectData::ReflexSystem(reflex) = &object.data else {
                        panic!("wrong inline class")
                    };
                    assert_eq!(
                        (reflex.ball_joints.len(), reflex.bone_name_hashes.len()),
                        (2, 6)
                    );
                }
            }
            Err(error) => {
                assert!(
                    !supported_names.contains(&resource.name.as_str()),
                    "{}: {error}",
                    resource.name
                );
                unsupported += 1;
                assert!(
                    error.to_string().contains("unsupported")
                        || (resource.name == "Addon_Human_Sword_Tag"
                            && error.to_string().contains("nonfinite")),
                    "{}: {error}",
                    resource.name
                );
            }
        }
    }
    assert_eq!((parsed, unsupported), (20, 8));
    Ok(())
}
