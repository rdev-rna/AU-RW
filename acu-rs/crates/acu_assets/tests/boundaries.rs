use acu_assets::{Limits, frame, package, reader::Reader};

fn raw_frame(data: &[u8]) -> Vec<u8> {
    let mut result = frame::MAGIC.to_le_bytes().to_vec();
    result.extend_from_slice(&1u16.to_le_bytes());
    result.push(1);
    result.extend_from_slice(&0x80008000u32.to_le_bytes());
    result.extend_from_slice(&1u32.to_le_bytes());
    result.extend_from_slice(&(data.len() as u16).to_le_bytes());
    result.extend_from_slice(&(data.len() as u16).to_le_bytes());
    // Adler-32 seeded with zero; independent straightforward fixture calculation.
    let (mut a, mut b) = (0u32, 0u32);
    for byte in data {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    result.extend_from_slice(&(b << 16 | a).to_le_bytes());
    result.extend_from_slice(data);
    result
}

#[test]
fn independent_lzo_literal_and_checksum() {
    // A handwritten LZO literal stream for ABC, not produced by our decoder.
    let encoded = [20, b'A', b'B', b'C', 17, 0, 0];
    let mut b = frame::MAGIC.to_le_bytes().to_vec();
    b.extend_from_slice(&[1, 0, 1, 0, 128, 0, 128, 1, 0, 0, 0, 3, 0, 7, 0]);
    // Checked independently with Python zlib.adler32(encoded,0).
    b.extend_from_slice(&[235, 0, 155, 4]);
    b.extend_from_slice(&encoded);
    let (decoded, info) = frame::decode(&mut Reader::new(&b), Limits::default()).unwrap();
    assert_eq!(decoded, b"ABC");
    assert!(info.checksum_matches_compressed);
}

#[test]
fn truncated_corrupt_and_over_limit_frames_fail() {
    let b = raw_frame(b"ABC");
    for end in 0..b.len() {
        assert!(
            frame::decode(&mut Reader::new(&b[..end]), Limits::default()).is_err(),
            "prefix {end}"
        );
    }
    let mut corrupt = b.clone();
    corrupt[27] ^= 1;
    assert!(frame::decode(&mut Reader::new(&corrupt), Limits::default()).is_err());
    let limits = Limits {
        max_decoded: 2,
        ..Limits::default()
    };
    assert!(frame::decode(&mut Reader::new(&b), limits).is_err());
    let mut count = b;
    count[15..19].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(frame::decode(&mut Reader::new(&count), Limits::default()).is_err());
}

#[test]
fn metadata_identity_record_bounds_and_opaque_status() {
    let mut metadata = 1u16.to_le_bytes().to_vec();
    metadata.extend_from_slice(&0xfedcba9876543210u64.to_le_bytes());
    metadata.extend_from_slice(&17u32.to_le_bytes());
    metadata.extend_from_slice(&0u16.to_le_bytes());
    let mut record = 0x12345678u32.to_le_bytes().to_vec();
    record.extend_from_slice(&3u32.to_le_bytes());
    record.extend_from_slice(&1u32.to_le_bytes());
    record.extend_from_slice(b"X\0ABC");
    let mut raw = raw_frame(&metadata);
    raw.extend_from_slice(&raw_frame(&record));
    let p = package::decode(&raw, Limits::default()).unwrap();
    let r = &p.resources[0];
    assert_eq!(
        (r.file_id.as_str(), r.type_id.as_str(), r.name.as_str()),
        ("fedcba9876543210", "12345678", "X")
    );
    assert_eq!(&p.bytes[r.payload.clone()], b"ABC");
    assert_eq!(r.typed_status, "unsupported");
    let mut tagged = metadata.clone();
    tagged[14..16].copy_from_slice(&1u16.to_le_bytes());
    tagged.extend_from_slice(&7u16.to_le_bytes());
    let mut tagged_raw = raw_frame(&tagged);
    tagged_raw.extend_from_slice(&raw_frame(&record));
    let parsed = package::decode(&tagged_raw, Limits::default()).unwrap();
    assert_eq!(parsed.resources[0].metadata_tags, vec![7]);
    assert!(parsed.metadata_extension.is_empty());
    raw.push(0);
    assert!(package::decode(&raw, Limits::default()).is_err());
}

#[test]
fn default_mode_stops_before_next_frame_and_checks_size() {
    let mut b = frame::MAGIC.to_le_bytes().to_vec();
    b.extend_from_slice(&[1, 0, 0, 0, 128, 0, 0]);
    b.push(1);
    b.extend_from_slice(&7u32.to_le_bytes());
    b.extend_from_slice(&3u32.to_le_bytes());
    b.extend_from_slice(&0x049b00ebu32.to_le_bytes());
    b.extend_from_slice(&[20, b'A', b'B', b'C', 17, 0, 0]);
    b.extend_from_slice(&raw_frame(b"XY"));
    let mut r = Reader::new(&b);
    let (first, _) = frame::decode(&mut r, Limits::default()).unwrap();
    assert_eq!(first, b"ABC");
    assert_eq!(r.position, 35);
    let (second, _) = frame::decode(&mut r, Limits::default()).unwrap();
    assert_eq!(second, b"XY");
    assert_eq!(r.remaining(), 0);
    b[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(frame::decode(&mut Reader::new(&b), Limits::default()).is_err());
}

#[test]
fn wide_length_toc_variants_are_bounded() {
    for version in [0u16, 2, 3] {
        let mut b = frame::MAGIC.to_le_bytes().to_vec();
        b.extend_from_slice(&version.to_le_bytes());
        b.push(1);
        b.extend_from_slice(&0x80008000u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&0x018a00c6u32.to_le_bytes());
        b.extend_from_slice(b"ABC");
        assert_eq!(
            frame::decode(&mut Reader::new(&b), Limits::default())
                .unwrap()
                .0,
            b"ABC"
        );
        b[19..23].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(frame::decode(&mut Reader::new(&b), Limits::default()).is_err());
    }
}
