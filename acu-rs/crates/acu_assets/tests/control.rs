use acu_assets::{
    Limits,
    control::{self, MetaValue},
};

fn fixture() -> Vec<u8> {
    // Version1, CodeVersion(u32), FileGUID(text), ContentPackagesList(u64 array).
    let mut b = Vec::new();
    for n in [1u32, 14, 0, 17, 19, 1, 3] {
        b.extend(n.to_le_bytes());
    }
    b.extend(b"abc\0");
    for n in [21u32, 6, 16] {
        b.extend(n.to_le_bytes());
    }
    b.extend(0x1122334455667788u64.to_le_bytes());
    b.extend(u64::MAX.to_le_bytes());
    b.extend(0u32.to_le_bytes());
    b
}

#[test]
fn typed_metadata_preserves_values_and_hex64() {
    let meta = control::parse_metadata(&fixture(), Limits::default()).unwrap();
    assert_eq!(meta.attributes.len(), 3);
    assert_eq!(meta.attributes[0].name, "CodeVersion");
    assert_eq!(meta.attributes[0].value, MetaValue::U32(17));
    assert_eq!(meta.attributes[1].value, MetaValue::Text("abc".to_owned()));
    assert_eq!(
        meta.attributes[2].value,
        MetaValue::U64Array(vec!["1122334455667788".into(), "ffffffffffffffff".into()])
    );
}

#[test]
fn metadata_rejects_truncation_bad_lengths_unknowns_and_limits() {
    let good = fixture();
    for end in 0..good.len() {
        assert!(
            control::parse_metadata(&good[..end], Limits::default()).is_err(),
            "end={end}"
        );
    }
    for (offset, replacement) in [(0, 2u32), (4, 9999), (8, 5), (40, 15)] {
        let mut b = good.clone();
        b[offset..offset + 4].copy_from_slice(&replacement.to_le_bytes());
        assert!(
            control::parse_metadata(&b, Limits::default()).is_err(),
            "offset={offset}"
        );
    }
    let mut b = good.clone();
    b[31] = 1;
    assert!(control::parse_metadata(&b, Limits::default()).is_err());
    let mut b = good.clone();
    b.push(0);
    assert!(control::parse_metadata(&b, Limits::default()).is_err());
    assert!(
        control::parse_metadata(
            &good,
            Limits {
                max_resources: 1,
                ..Limits::default()
            }
        )
        .is_err()
    );
    assert!(
        control::parse_metadata(
            &good,
            Limits {
                max_decoded: 1,
                ..Limits::default()
            }
        )
        .is_err()
    );
}
