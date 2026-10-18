use acu_assets::texture;

fn fixture() -> Vec<u8> {
    let mut b = vec![1];
    b.extend_from_slice(&42u64.to_le_bytes());
    b.extend_from_slice(&0xa2b7e917u32.to_le_bytes());
    for n in [1u32, 1, 1, 0, 1, 1, 1, 11, 5] {
        b.extend_from_slice(&n.to_le_bytes());
    }
    b.extend_from_slice(&[0, 0]);
    b.extend_from_slice(&0u32.to_le_bytes());
    b.push(0);
    b.extend_from_slice(&0u64.to_le_bytes());
    b.push(0);
    b.extend_from_slice(&0u64.to_le_bytes());
    b.extend_from_slice(&0x13237fe9u32.to_le_bytes());
    for n in [1u32, 7, 1, 1, 1, 1, 0, 1, 1, 0, 4, 0, 4] {
        b.extend_from_slice(&n.to_le_bytes());
    }
    b.extend_from_slice(&[20, 40, 80, 255]);
    b
}

#[test]
fn authored_pixel_and_dimensions() {
    let b = fixture();
    let t = texture::parse(&b, "000000000000002a").unwrap();
    assert_eq!((t.width, t.height, t.dxgi_format), (1, 1, 87));
    assert_eq!(t.mips, vec![0..4]);
    assert_eq!(t.pixels, &[20, 40, 80, 255]);
}

#[test]
fn corruption_truncation_and_unresolved_references_fail() {
    let b = fixture();
    for end in 0..b.len() {
        assert!(
            texture::parse(&b[..end], "000000000000002a").is_err(),
            "prefix {end}"
        );
    }
    for at in [1, 13, 25, 56, 73, 85, 125] {
        let mut changed = b.clone();
        changed[at] ^= 0x80;
        assert!(
            texture::parse(&changed, "000000000000002a").is_err(),
            "offset {at}"
        );
    }
    let mut extra = b;
    extra.push(0);
    assert!(texture::parse(&extra, "000000000000002a").is_err());
}

fn variant(
    width: u32,
    height: u32,
    depth: u32,
    kind: u32,
    levels: u32,
    format: u32,
    length: u32,
) -> Vec<u8> {
    let mut b = fixture();
    for (at, value) in [
        (13, width),
        (17, height),
        (21, depth),
        (25, format),
        (29, kind),
        (37, levels),
        (85, width),
        (89, height),
        (93, depth),
        (97, levels),
        (101, format),
        (105, kind),
        (125, length),
    ] {
        b[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    b.resize(129 + length as usize, 0);
    b
}

#[test]
fn native_kind_mip_and_block_dimensions() {
    let one = variant(8, 1, 1, 0, 3, 0, 56);
    let t = texture::parse(&one, "000000000000002a").unwrap();
    assert_eq!((t.kind, t.depth, t.layers), (0, 1, 1));
    assert_eq!(t.mips, vec![0..32, 32..48, 48..56]);

    let cube = variant(4, 4, 1, 2, 2, 0, 480);
    let t = texture::parse(&cube, "000000000000002a").unwrap();
    assert_eq!(t.layers, 6);
    assert_eq!(
        t.mips,
        vec![
            0..64,
            64..80,
            80..144,
            144..160,
            160..224,
            224..240,
            240..304,
            304..320,
            320..384,
            384..400,
            400..464,
            464..480
        ]
    );

    let volume = variant(4, 2, 2, 3, 3, 0, 76);
    let t = texture::parse(&volume, "000000000000002a").unwrap();
    assert_eq!((t.kind, t.depth, t.layers), (3, 2, 1));
    assert_eq!(t.mips, vec![0..64, 64..72, 72..76]);

    let mut bc = variant(1, 8, 1, 1, 2, 2, 24);
    bc[85..89].copy_from_slice(&4u32.to_le_bytes());
    let t = texture::parse(&bc, "000000000000002a").unwrap();
    assert_eq!((t.width, t.compiled_width, t.dxgi_format), (1, 4, 71));
    assert_eq!(t.mips, vec![0..16, 16..24]);
}

#[test]
fn invalid_kinds_shapes_and_variant_lengths_fail() {
    for b in [
        variant(4, 2, 1, 0, 1, 0, 32),  // 1D cannot have height 2.
        variant(4, 2, 1, 2, 1, 0, 192), // Cube must be square.
        variant(4, 4, 2, 1, 1, 0, 128), // 2D cannot have depth 2.
        variant(4, 4, 0, 3, 1, 0, 0),
        variant(4, 4, 1, 4, 1, 0, 64),
        variant(4, 4, 1, 2, 1, 0, 64),  // Five cube faces missing.
        variant(4, 4, 4, 3, 2, 0, 260), // Volume mip depth must shrink.
    ] {
        assert!(texture::parse(&b, "000000000000002a").is_err());
    }
    for (format, compiled_width) in [(0, 4u32), (2, 8)] {
        let mut b = variant(1, 8, 1, 1, 1, format, 32);
        b[85..89].copy_from_slice(&compiled_width.to_le_bytes());
        assert!(texture::parse(&b, "000000000000002a").is_err());
    }
    let b = variant(4, 4, 1, 2, 2, 0, 480);
    for end in 0..b.len() {
        assert!(texture::parse(&b[..end], "000000000000002a").is_err());
    }
}

#[test]
fn dynamic_placeholder_is_typed_but_has_no_uploadable_pixels() {
    let mut b = fixture();
    b.truncate(65);
    b[49] = 1; // Dynamic.
    b[64] = 3; // Observed no-inline-data marker.
    let texture::TextureResource::NoInlinePixels(header) =
        texture::parse_resource(&b, "000000000000002a").unwrap()
    else {
        panic!("placeholder incorrectly supplied pixels");
    };
    assert_eq!(
        (header.width, header.height, header.compiled_marker),
        (1, 1, 3)
    );
    assert!(header.dynamic);
    assert!(header.top_mip_id.is_none());
    assert!(texture::parse(&b, "000000000000002a").is_err());
    for end in 0..b.len() {
        assert!(texture::parse_resource(&b[..end], "000000000000002a").is_err());
    }
    for (at, value) in [(49, 0), (49, 2), (56, 1), (64, 1), (64, 2)] {
        let mut changed = b.clone();
        changed[at] = value;
        assert!(texture::parse_resource(&changed, "000000000000002a").is_err());
    }
    b.push(0);
    assert!(texture::parse_resource(&b, "000000000000002a").is_err());

    let b = fixture();
    assert!(matches!(
        texture::parse_resource(&b, "000000000000002a").unwrap(),
        texture::TextureResource::Resident(_)
    ));
    let mut external = b;
    external[56] = 5;
    assert!(texture::parse_resource(&external, "000000000000002a").is_err());
}

#[test]
#[ignore = "requires ACU_GAME_DIR; native LZO identity in ACU-RE-0007"]
fn installed_dynamic_placeholder() -> acu_assets::Result<()> {
    use acu_assets::{Limits, forge::Forge, package, sha256};
    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: set ACU_GAME_DIR")?;
    let mut archive = Forge::open(
        std::path::Path::new(&root).join("DataPC.forge"),
        Limits::default(),
    )?;
    let raw = archive.raw(1478)?;
    assert_eq!(
        sha256(&raw),
        "26a4a401fa74f3fa69712d300779f2063eacd907379c63f42c508b59d4e6b69a"
    );
    let package = package::decode(&raw, Limits::default())?;
    assert_eq!(
        sha256(&package.bytes),
        "71a2fad1bf9aafcf3333bef5988eb43dd746277d6de5377472059e95728d153a"
    );
    let resource = &package.resources[0];
    let payload = &package.bytes[resource.payload.clone()];
    assert_eq!(resource.file_id, "00000001a36b095f");
    assert_eq!(
        sha256(payload),
        "5a9408727f2ccdb5c8cef15ee5b180247509fded358b39021a6f4ad0e9bd4fc8"
    );
    assert_eq!(payload.len(), 65);
    let texture::TextureResource::NoInlinePixels(header) =
        texture::parse_resource(payload, &resource.file_id)?
    else {
        panic!("dynamic font placeholder incorrectly supplied pixels");
    };
    assert_eq!(
        (
            header.width,
            header.height,
            header.depth,
            header.kind,
            header.engine_format
        ),
        (2048, 2048, 1, 1, 9)
    );
    assert_eq!(header.user_category, 8);
    assert_eq!(header.compiled_marker, 3);
    assert!(header.dynamic);
    assert!(texture::parse(payload, &resource.file_id).is_err());
    Ok(())
}

#[test]
#[ignore = "requires ACU_GAME_DIR; original-byte identities in ACU-RE-0005"]
fn installed_texture_variants() -> acu_assets::Result<()> {
    use acu_assets::{Limits, forge::Forge, package, sha256};
    let root = std::env::var("ACU_GAME_DIR").map_err(|_| "unavailable: set ACU_GAME_DIR")?;
    let mut archive = Forge::open(
        std::path::Path::new(&root).join("DataPC.forge"),
        Limits::default(),
    )?;
    for (index, stored_hash, payload_hash, kind, dimensions, levels, length) in [
        (
            8,
            "9c9e0f8a0990e83a5b7dbddbdc0164e14c7ba8181f0bd39bef4c06f5b2f657d6",
            "5a60616386181658c545039357bf92fad463bcb856baa4556ee1485272587018",
            0,
            (256, 1, 1),
            1,
            1024,
        ),
        (
            9,
            "5e7303edfc02759339d15b715d05aca04a2048693c12064a82a11c6e5a2c6f71",
            "47225d4e3430bc3a15cfc77f26a76c5e28cba559898fe76c833c1789f9c81a19",
            2,
            (4, 4, 1),
            1,
            384,
        ),
        (
            44,
            "75184a6f32b9ed826a28a1d916c1ad8ac476db80a8b4d8f264272383d8f97ec5",
            "7fff1112b1a686587a03921b857376c717f73f7b0bc88aa3befec41cd287d8d9",
            3,
            (4, 4, 4),
            1,
            256,
        ),
        (
            316,
            "7e0c265120fbdca6f03e2c5ce1874334352fec3d9ea4a7c7c103bcbf177ac3e9",
            "4f24a53c0cf32c3f229a314740889804f64e776e5b00aaf9ce8ea27a3b8e121f",
            1,
            (1, 1024, 1),
            1,
            4096,
        ),
        (
            317,
            "f50275dfb32eac4c4aa5fbc41a02a04bf3a6168af5acb0bd1a3aa09fea7eafb5",
            "531a50335808bd9bca1effc1395e0260a5383088deea6ac1db2fc8d2ef4ec7de",
            1,
            (1, 1024, 1),
            1,
            2048,
        ),
        (
            327,
            "b7cd590b04c3fc226ead7f431d470109c6162fee1c8d1721c17f958b51351dae",
            "a143444cee59cae9102537d3fa7f9689f99b1eba134cbd81010488df836ceb03",
            3,
            (32, 32, 32),
            6,
            149796,
        ),
    ] {
        let raw = archive.raw(index)?;
        assert_eq!(sha256(&raw), stored_hash);
        let package = package::decode(&raw, Limits::default())?;
        let decoded_hash = match index {
            8 => "64493f9768a7cae96c8ebd3d75d8694b51f47e7bcb75aa4c9aaa0bbb6fc5536f",
            9 => "9fa31fef19dc59af58d73d4a08618d2d61ffba83a049be58c02fa4faa822f98e",
            44 => "f80eda4fb41f2dd2701dbd25c7ae6a4087ef09533f79ee02b337a7f13d6ccb75",
            316 => "1cd45e611f62595779351d3c0908e9435a170678200cd17cfe8bac84d680e035",
            317 => "f4649dd57c6cedc1dc96c0744e6f1368dd421a827d3b7886b7d0b642edc012ad",
            327 => "3cf8f9fd022947d0ef8506f875da754fd8c97ddddd3c4619922e74af6cf396c7",
            _ => unreachable!(),
        };
        assert_eq!(
            sha256(&package.bytes),
            decoded_hash,
            "independent native LZO oracle"
        );
        let resource = &package.resources[0];
        let payload = &package.bytes[resource.payload.clone()];
        assert_eq!(sha256(payload), payload_hash);
        let t = texture::parse(payload, &resource.file_id)?;
        assert_eq!(t.kind, kind);
        assert_eq!((t.width, t.height, t.depth), dimensions);
        assert_eq!(t.mip_count, levels);
        assert_eq!(t.pixels.len(), length);
        assert_eq!(t.mips.len(), (t.layers * t.mip_count) as usize);
        if index == 327 {
            assert_eq!(
                t.mips,
                vec![
                    0..131072,
                    131072..147456,
                    147456..149504,
                    149504..149760,
                    149760..149792,
                    149792..149796
                ]
            );
        }
        if index == 44 {
            assert!(t.pixels.iter().all(|byte| *byte == 255));
        }
    }
    Ok(())
}
