use acu_assets::material::{self, ObjectData, ParameterValue};
use acu_assets::{Limits, Result};

fn u32b(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

fn reference(id: u64, global: u8) -> Vec<u8> {
    let mut b = vec![1, global];
    b.extend_from_slice(&id.to_le_bytes());
    b
}

fn null_reference() -> Vec<u8> {
    vec![3, 0, 0, 0, 0, 0, 0, 0, 0, 0]
}

fn header(id: u64, class: u32) -> Vec<u8> {
    let mut b = vec![1];
    b.extend_from_slice(&id.to_le_bytes());
    b.extend_from_slice(&u32b(class));
    b
}

// Fixture offsets, derived from the layout below.
const MASK_HASH_AT: usize = 41;
const COUNT_AT: usize = 121;
const P0_NAME_AT: usize = 125;
const P2_HEADER_AT: usize = 177;
const P2_HASH_AT: usize = 202;
const P3_HEADER_AT: usize = 238;
const P3_KIND_AT: usize = 246;
const P3_HASH_AT: usize = 262;
const FIXTURE_LEN: usize = 301;

/// Material fixture: a vector, a float, a managed `TextureSelector` object and
/// an unmanaged `UVTransform` object.
fn fixture() -> Vec<u8> {
    let mut b = header(0x2a, 0x85c8_17c3);
    b.extend_from_slice(&reference(0x1000, 1)); // template
    b.extend_from_slice(&reference(0x2000, 0)); // texture set
    b.extend_from_slice(&0x3000u64.to_le_bytes()); // mask
    b.extend_from_slice(&u32b(0xdf5d_6c0e));
    b.extend_from_slice(&(0u64..8).map(|v| v as u8).collect::<Vec<_>>());
    b.extend_from_slice(&u32b(1)); // blend mode
    b.extend_from_slice(&u32b(2)); // alpha display mode
    b.push(128); // alpha test value
    for i in 0..17 {
        b.push((i % 2) as u8);
    }
    b.extend_from_slice(&null_reference()); // backface material
    b.extend_from_slice(&0x4000u64.to_le_bytes()); // match mask
    b.extend_from_slice(&u32b(0x92b9_5f74));
    b.extend_from_slice(&(0u8..16).collect::<Vec<_>>());
    b.extend_from_slice(&u32b(1)); // category
    assert_eq!(b.len(), COUNT_AT);
    b.extend_from_slice(&u32b(4)); // parameter count
    assert_eq!(b.len(), P0_NAME_AT);
    // 0: Vector4.
    b.extend_from_slice(&u32b(0x8ca9_8c4d));
    b.extend_from_slice(&u32b(0));
    b.extend_from_slice(&u32b(0x0d_0000));
    b.extend_from_slice(&u32b(0));
    for v in [1.0f32, 1.0, 1.0, 0.0] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    // 1: Float.
    b.extend_from_slice(&u32b(0xd3cd_1895));
    b.extend_from_slice(&u32b(0));
    b.extend_from_slice(&u32b(0x0a_0000));
    b.extend_from_slice(&u32b(0));
    b.extend_from_slice(&0.5f32.to_le_bytes());
    // 2: managed TextureSelector (leading byte, zero class ID).
    assert_eq!(b.len(), P2_HEADER_AT);
    b.extend_from_slice(&u32b(0x1371_c615));
    b.extend_from_slice(&u32b(0x7d08_460d));
    b.extend_from_slice(&u32b(0x13_0000));
    b.extend_from_slice(&u32b(0));
    b.push(1);
    b.extend_from_slice(&0u64.to_le_bytes());
    b.extend_from_slice(&u32b(0x7d08_460d));
    assert_eq!(b.len(), P2_HASH_AT + 4);
    b.extend_from_slice(&u32b(2)); // specification method
    b.extend_from_slice(&u32b(1)); // map type
    b.extend_from_slice(&u32b(0)); // frame
    b.extend_from_slice(&null_reference());
    b.extend_from_slice(&reference(0x5000, 1));
    // 3: unmanaged UVTransform.
    assert_eq!(b.len(), P3_HEADER_AT);
    b.extend_from_slice(&u32b(0xac06_3254));
    b.extend_from_slice(&u32b(0xc52e_2125));
    assert_eq!(b.len(), P3_KIND_AT);
    b.extend_from_slice(&u32b(0x13_0000));
    b.extend_from_slice(&u32b(0));
    b.extend_from_slice(&0u64.to_le_bytes());
    assert_eq!(b.len(), P3_HASH_AT);
    b.extend_from_slice(&u32b(0xc52e_2125));
    for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.push(1);
    b.push(0);
    b.push(9);
    assert_eq!(b.len(), FIXTURE_LEN);
    b
}

#[test]
fn authored_header_and_parameter_values() {
    let b = fixture();
    let m = material::parse(&b, "000000000000002a", Limits::default()).unwrap();
    assert_eq!(m.template.file_id, "0000000000001000");
    assert_eq!(m.template.global, 1);
    assert_eq!(m.texture_set.file_id, "0000000000002000");
    assert_eq!(m.mask.file_id, "0000000000003000");
    assert_eq!(m.mask.bits, [0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!((m.blend_mode, m.alpha_display_mode), (1, 2));
    assert_eq!(m.alpha_test_value, 128);
    assert!(m.flags.z_write_disabled_alpha);
    assert!(!m.flags.z_write_disabled_opaque);
    assert!(!m.flags.two_sided);
    assert!(!m.flags.mask_write_disabled);
    assert_eq!(m.backface_material.tag, 3);
    assert_eq!(m.match_mask.file_id, "0000000000004000");
    assert_eq!(m.match_mask.material_match_mask[15], 15);
    assert_eq!(m.category, 1);
    assert_eq!(m.parameters.len(), 4);
    assert_eq!(m.parameters[0].name_hash, "8ca98c4d");
    assert_eq!(
        m.parameters[0].value,
        ParameterValue::Vector4([1.0, 1.0, 1.0, 0.0])
    );
    assert_eq!(m.parameters[1].value, ParameterValue::Float(0.5));
    assert_eq!(m.parameters[2].kind, "00130000");
    let ParameterValue::Object(selector) = &m.parameters[2].value else {
        panic!("parameter 2 is not an object");
    };
    assert_eq!(selector.class_hash, "7d08460d");
    assert_eq!(selector.managed_flag, Some(1));
    let ObjectData::TextureSelector(selector) = &selector.data else {
        panic!("parameter 2 is not a texture selector");
    };
    assert_eq!(selector.specification_method, 2);
    assert_eq!(selector.map_type, 1);
    assert_eq!(selector.texture_set.tag, 3);
    assert_eq!(selector.texture_map.file_id, "0000000000005000");
    let ParameterValue::Object(uv) = &m.parameters[3].value else {
        panic!("parameter 3 is not an object");
    };
    assert_eq!(uv.managed_flag, None);
    let ObjectData::UvTransform(uv) = &uv.data else {
        panic!("parameter 3 is not a UV transform");
    };
    assert_eq!(uv.scale_u, 1.0);
    assert_eq!(uv.rotation, 7.0);
    assert!(uv.animate_translation);
    assert!(!uv.animate_rotation);
    assert_eq!(uv.unk01, 9);
    assert_eq!(m.texture_selector(1).unwrap().map_type, 1);
    assert!(m.texture_selector(0).is_none());
    assert_eq!(material::texture_map_type_name(0), Some("MapDiffuse"));
    assert_eq!(material::texture_map_type_name(7), Some("MapOcclusion"));
    assert_eq!(material::texture_map_type_name(11), Some("RVBmask"));
    assert_eq!(material::texture_map_type_name(12), Some("MapMaxNbTypes"));
    assert_eq!(material::texture_map_type_name(13), None);
}

#[test]
fn truncation_trailing_and_unsupported_variants_fail() {
    let b = fixture();
    for end in 0..b.len() {
        assert!(
            material::parse(&b[..end], "000000000000002a", Limits::default()).is_err(),
            "prefix {end}"
        );
    }
    let mut extra = b.clone();
    extra.push(0);
    assert!(material::parse(&extra, "000000000000002a", Limits::default()).is_err());
    assert!(material::parse(&b, "000000000000002b", Limits::default()).is_err());
    let mut wrapper = b.clone();
    wrapper[0] = 2;
    assert!(material::parse(&wrapper, "000000000000002a", Limits::default()).is_err());

    // Mask, match-mask and class hashes are checked.
    for at in [MASK_HASH_AT, 97] {
        let mut changed = b.clone();
        changed[at] ^= 0x01;
        assert!(material::parse(&changed, "000000000000002a", Limits::default()).is_err());
    }
    // An unknown parameter kind must fail clearly.
    let mut unknown_kind = b.clone();
    unknown_kind[P3_KIND_AT..P3_KIND_AT + 4].copy_from_slice(&u32b(0x0b_0000));
    assert!(material::parse(&unknown_kind, "000000000000002a", Limits::default()).is_err());
    // A float parameter cannot carry an object class hash; the declared data
    // type alone must not be used for a registered object kind.
    let mut cross_kind = b.clone();
    cross_kind[P3_KIND_AT..P3_KIND_AT + 4].copy_from_slice(&u32b(0x0a_0000));
    assert!(material::parse(&cross_kind, "000000000000002a", Limits::default()).is_err());
    // An unknown object class must fail with its hash, not guess a layout.
    let mut unknown_class = b.clone();
    unknown_class[P3_HASH_AT..P3_HASH_AT + 4].copy_from_slice(&u32b(0xdead_beef));
    assert!(material::parse(&unknown_class, "000000000000002a", Limits::default()).is_err());
    // A managed flag whose class identity disagrees with the data type fails.
    let mut bad_identity = b.clone();
    bad_identity[P2_HASH_AT..P2_HASH_AT + 4].copy_from_slice(&u32b(0x1234_5678));
    assert!(material::parse(&bad_identity, "000000000000002a", Limits::default()).is_err());
    // Parameter names are opaque; an unused name hash makes no difference.
    let mut renamed = b.clone();
    renamed[P0_NAME_AT..P0_NAME_AT + 4].copy_from_slice(&u32b(0));
    assert!(material::parse(&renamed, "000000000000002a", Limits::default()).is_ok());
}

#[test]
fn parameter_count_limit_is_enforced() {
    let mut b = fixture();
    b[COUNT_AT..COUNT_AT + 4].copy_from_slice(&u32b(1_000_000));
    assert!(material::parse(&b, "000000000000002a", Limits::default()).is_err());
    let limits = Limits {
        max_resources: 3,
        ..Limits::default()
    };
    let b = fixture();
    assert!(material::parse(&b, "000000000000002a", limits).is_err());
}

#[test]
fn file_reference_scope_is_a_byte() {
    let mut bytes = fixture();
    bytes[24] = 4; // TextureSet reference scope, as in the native blade vector.
    let parsed = material::parse(&bytes, "000000000000002a", Limits::default()).unwrap();
    assert_eq!(parsed.texture_set.global, 4);
    assert_eq!(parsed.texture_set.file_id, "0000000000002000");
}

/// Texture-set fixture: twelve roles, a skipped byte and the source ID.
fn texture_set_fixture() -> Vec<u8> {
    let mut b = header(0x60, 0xd70e_6670);
    for role in 0..12u64 {
        if role % 4 == 3 {
            b.extend_from_slice(&null_reference());
        } else {
            b.extend_from_slice(&reference(0x7000 + role, 1));
        }
    }
    b.push(0);
    b.extend_from_slice(&0x8000u64.to_le_bytes());
    b
}

#[test]
fn authored_texture_set_roles() {
    let b = texture_set_fixture();
    assert_eq!(b.len(), 13 + 120 + 9);
    let set = material::parse_texture_set(&b, "0000000000000060", Limits::default()).unwrap();
    assert_eq!(set.diffuse.file_id, "0000000000007000");
    assert_eq!(set.normal.file_id, "0000000000007001");
    assert_eq!(set.specular.file_id, "0000000000007002");
    assert_eq!(set.offset_bump.tag, 3);
    assert_eq!(set.emissive.file_id, "0000000000007004");
    assert_eq!(set.transmission.file_id, "0000000000007005");
    assert_eq!(set.occlusion.file_id, "0000000000007006");
    assert_eq!(set.mask1.tag, 3);
    assert_eq!(set.mask2.file_id, "0000000000007008");
    assert_eq!(set.cookie.file_id, "0000000000007009");
    assert_eq!(set.env_lighting.file_id, "000000000000700a");
    assert_eq!(set.generic.tag, 3);
    assert_eq!(set.source_prefix, 0);
    assert_eq!(set.source_id, "0000000000008000");
    for end in 0..b.len() {
        assert!(
            material::parse_texture_set(&b[..end], "0000000000000060", Limits::default()).is_err(),
            "prefix {end}"
        );
    }
    let mut extra = b.clone();
    extra.push(0);
    assert!(material::parse_texture_set(&extra, "0000000000000060", Limits::default()).is_err());
    assert!(
        material::parse_texture_set(&b, "0000000000000061", Limits::default()).is_err(),
        "identity must be checked"
    );
    let mut bad_tag = texture_set_fixture();
    bad_tag[13] = 2;
    assert!(material::parse_texture_set(&bad_tag, "0000000000000060", Limits::default()).is_err());
    // A material payload is not a texture set.
    assert!(
        material::parse_texture_set(&fixture(), "000000000000002a", Limits::default()).is_err(),
        "cross-class payloads must be rejected"
    );
}

/// Arno coat pieces in `DataPC_ACU_LGS_BelleEpoque.forge` entry 679 and the
/// texture sets they name; identities and hashes are in ACU-RE-0013.
#[test]
#[ignore = "requires ACU_GAME_DIR; installed payload identities in ACU-RE-0013"]
fn installed_material_texture_chain() -> Result<()> {
    use acu_assets::{forge::Forge, package, sha256};
    use std::path::Path;

    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: ACU_GAME_DIR is unset")?;
    let mut forge = Forge::open(
        Path::new(&root).join("DataPC_ACU_LGS_BelleEpoque.forge"),
        Limits::default(),
    )?;
    let raw = forge.raw(679)?;
    let package = package::decode(&raw, Limits::default())?;
    let resource = |wanted: &str| -> Result<usize> {
        package
            .resources
            .iter()
            .position(|r| r.file_id == wanted)
            .ok_or_else(|| format!("entry 679 has no resource {wanted}").into())
    };
    for (
        material_id,
        material_name,
        payload_hash,
        parameters,
        set_id,
        set_hash,
        diffuse_id,
        normal_id,
        mask1_id,
        source_id,
    ) in [
        (
            "0000000b1b911534",
            "CN_P_Arnaud_SQ4",
            "29f787cffa2cb89fc6ec9827cf544369b119939f6e6dfeab6e5114d608970988",
            37,
            "0000000b1b911510",
            "49dc8512c10a56e0cf4f1f6f359bf5352abb27dbe8b937b4eaf8b404eecf795e",
            "0000000b1b911514",
            "0000000b1b911518",
            "0000000ce14d8720",
            "0000000b1b91150f",
        ),
        (
            "0000000b1b911533",
            "CN_P_Arnaud_SQ3",
            "2e4fc62575dfeb364eadd24c6bac540f4c26d1ea16f061c8a2ee1f3a0d052bc2",
            41,
            "0000000b1b911503",
            "7ff7db9d66b92596271be41cae764365602638f2e01676b48f9bc3c4b3a056f5",
            "0000000b1b911507",
            "0000000b1b91150b",
            "0000000ce14d8743",
            "0000000b1b911502",
        ),
        (
            "0000000b1b911530",
            "CN_P_Arnaud_SQ1",
            "35e9a2d8eabf78fb1fc727ed0140336c987767688f222b4b6af480dc975861f5",
            41,
            "0000000b1b9114ea",
            "686686bd623318289d3a8be70f4324d97db56bc9cd304cdc614aa23269a20ecd",
            "0000000b1b9114ee",
            "0000000b1b9114f2",
            "0000000ce14d86f5",
            "0000000b1b9114e9",
        ),
    ] {
        let index = resource(material_id)?;
        let r = &package.resources[index];
        assert_eq!(r.name, material_name);
        let payload = &package.bytes[r.payload.clone()];
        assert_eq!(sha256(payload), payload_hash, "{material_id}");
        let m = material::parse(payload, &r.file_id, Limits::default())?;
        assert_eq!(m.texture_set.file_id, set_id);
        assert_eq!(m.texture_set.global, 0);
        assert_eq!(m.parameters.len(), parameters);
        assert_eq!(m.mask.bits, [0; 8]);
        assert_eq!(m.match_mask.file_id, "0000000000000000");
        assert_eq!(m.category, 1);
        assert_eq!(m.backface_material.tag, 3);
        // Map type 0 and 1 agree with the texture set's diffuse and normal
        // roles; map type 7 names the same map the set stores as Mask1.
        let diffuse = m.texture_selector(0).expect("diffuse selector");
        let normal = m.texture_selector(1).expect("normal selector");
        assert_eq!(diffuse.texture_map.file_id, diffuse_id);
        assert_eq!(normal.texture_map.file_id, normal_id);
        assert_eq!(diffuse.specification_method, 0);
        assert_eq!(diffuse.texture_set.tag, 3);
        let mask = m.texture_selector(7).expect("map type 7 selector");
        assert_eq!(mask.texture_map.file_id, mask1_id);
        assert!(
            m.texture_selector(8).is_none(),
            "MapMask1 is not serialized"
        );
        // Every object parameter in these materials is a selector or a UV
        // transform at a known offset; the full payload must be consumed.
        assert_eq!(m.parameters[32].kind, "00130000");
        let ParameterValue::Object(uv) = &m.parameters[32].value else {
            panic!("parameter 32 is not an object");
        };
        assert_eq!(uv.class_hash, "c52e2125");
        assert_eq!(uv.managed_flag, None);

        let set_index = resource(set_id)?;
        let set_resource = &package.resources[set_index];
        let set_payload = &package.bytes[set_resource.payload.clone()];
        assert_eq!(sha256(set_payload), set_hash, "{set_id}");
        assert_eq!(set_payload.len(), 142);
        let set =
            material::parse_texture_set(set_payload, &set_resource.file_id, Limits::default())?;
        assert_eq!(set.diffuse.file_id, diffuse_id);
        assert_eq!(set.normal.file_id, normal_id);
        assert_eq!(set.mask1.file_id, mask1_id);
        assert_eq!(set.source_prefix, 0);
        assert_eq!(set.source_id, source_id);
        for role in [
            &set.specular,
            &set.offset_bump,
            &set.emissive,
            &set.transmission,
            &set.occlusion,
            &set.mask2,
            &set.cookie,
            &set.env_lighting,
            &set.generic,
        ] {
            assert_eq!(role.tag, 3, "unset roles stay null");
        }
    }
    // A separate native carrier disproves treating IsGlobal as a boolean.
    let mut forge = Forge::open(Path::new(&root).join("DataPC.forge"), Limits::default())?;
    let raw = forge.raw(110)?;
    assert_eq!(
        sha256(&raw),
        "7d5f500049eeb22158005bbd74fb8c22b84b84501d226aa6bfc66c4b0aa2334f"
    );
    let package = package::decode(&raw, Limits::default())?;
    let resource = &package.resources[12269];
    assert_eq!(resource.file_id, "00000013c1e4eb48");
    let payload = &package.bytes[resource.payload.clone()];
    assert_eq!(
        sha256(payload),
        "ca3e68b3c6dc8355b2eee16777b6e07c8a37686a8775ac8f89c7f039341b92e1"
    );
    let blade = material::parse(payload, &resource.file_id, Limits::default())?;
    assert_eq!(blade.texture_set.global, 4);
    assert_eq!(blade.texture_set.file_id, "000000131c3be1b0");
    Ok(())
}

/// The three texture maps behind the first material, from the archive that
/// stores them; pixel hashes cover every stored mip level.
#[test]
#[ignore = "requires ACU_GAME_DIR; installed texture identities in ACU-RE-0013"]
fn installed_arno_texture_maps() -> Result<()> {
    use acu_assets::{forge::Forge, package, sha256, texture};
    use std::path::Path;

    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: ACU_GAME_DIR is unset")?;
    let mut forge = Forge::open(
        Path::new(&root).join("DataPC_SharedGroup_00.forge"),
        Limits::default(),
    )?;
    for (entry, file_id, name, map_type, dxgi, dimension, levels, payload_hash, pixel_hash) in [
        (
            626,
            "0000000ce14d8720",
            "CN_P_Arnaud_sq4_Mask1Map",
            7,
            71,
            512,
            10,
            "1bafdad08b03e437518d68febe631c1e975fc1e623697b522213d74bb0e7b675",
            "37b6c8c286005ca6b8f52cc5584c2826f7a82c558268691e558106e8e5d86095",
        ),
        (
            627,
            "0000000b1b911518",
            "CN_P_Arnaud_sq4_NormalMap",
            1,
            98,
            1024,
            11,
            "eb6ac5b39e24f97223024098f6fba975bccf86e4b8352ffa54a4ee1d62a051d6",
            "1373fbd2e696439556df8637a1a5ee358da594cda705dbfcfc01dee8997d8bff",
        ),
        (
            628,
            "0000000b1b911514",
            "CN_P_Arnaud_sq4_DiffuseMap",
            0,
            71,
            1024,
            11,
            "dab4b9bc3f8bbc8ad63be99f1abe463bf75f1f0cc4f11c2eb766a568c011f2ae",
            "51aae5ca2510697727a45644631058def6e41ade4d0bba841fc9ed66564ddba7",
        ),
    ] {
        let raw = forge.raw(entry)?;
        let package = package::decode(&raw, Limits::default())?;
        let r = &package.resources[package
            .resources
            .iter()
            .position(|r| r.file_id == file_id)
            .ok_or("resource missing")?];
        assert_eq!(r.name, name);
        let payload = &package.bytes[r.payload.clone()];
        assert_eq!(sha256(payload), payload_hash);
        let t = texture::parse(payload, &r.file_id)?;
        assert_eq!(t.dxgi_format, dxgi);
        assert_eq!((t.width, t.height), (dimension, dimension));
        assert_eq!(t.mip_count, levels);
        assert_eq!(t.mips.len(), levels as usize);
        // The texture's own map type agrees with the material selector that
        // names it: 0 diffuse, 1 normal, 7 the map stored as Mask1.
        assert_eq!(t.map_type, map_type);
        assert_eq!(sha256(t.pixels), pixel_hash, "entry {entry}");
    }
    Ok(())
}
