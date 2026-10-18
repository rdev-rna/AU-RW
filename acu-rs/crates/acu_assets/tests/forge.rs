use acu_assets::{Limits, forge::Forge};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);

fn fixture() -> Vec<u8> {
    let mut b = vec![0; 0x500];
    b[..9].copy_from_slice(b"scimitar\0");
    b[9..13].copy_from_slice(&27u32.to_le_bytes());
    b[13..21].copy_from_slice(&0x41au64.to_le_bytes());
    b[0x41a..0x41e].copy_from_slice(&1u32.to_le_bytes());
    b[0x43e..0x446].copy_from_slice(&0x446u64.to_le_bytes());
    b[0x446..0x44a].copy_from_slice(&1u32.to_le_bytes());
    b[0x44e..0x456].copy_from_slice(&0x476u64.to_le_bytes());
    b[0x456..0x45e].copy_from_slice(&u64::MAX.to_le_bytes());
    b[0x476..0x47e].copy_from_slice(&0x4fcu64.to_le_bytes());
    b[0x47e..0x486].copy_from_slice(&0xfedcba9876543210u64.to_le_bytes());
    b[0x486..0x48a].copy_from_slice(&4u32.to_le_bytes());
    b[0x4fc..].copy_from_slice(b"ABCD");
    b
}

fn check(b: &[u8], success: bool) {
    let path = std::env::temp_dir().join(format!(
        "acu-forge-test-{}-{}.bin",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    use std::io::Write;
    file.write_all(b).unwrap();
    drop(file);
    let result = Forge::open(&path, Limits::default());
    assert_eq!(result.is_ok(), success);
    if let Ok(mut f) = result {
        assert_eq!(f.entries[0].file_id, "fedcba9876543210");
        assert_eq!(f.raw(0).unwrap(), b"ABCD");
        assert!(f.raw(1).is_err());
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn expected_record_and_invalid_tables() {
    let b = fixture();
    check(&b, true);
    let mut cycle = b.clone();
    cycle[0x456..0x45e].copy_from_slice(&0x446u64.to_le_bytes());
    check(&cycle, false);
    let mut overflow = b.clone();
    overflow[0x476..0x47e].copy_from_slice(&u64::MAX.to_le_bytes());
    check(&overflow, false);
    let mut huge = b.clone();
    huge[0x446..0x44a].copy_from_slice(&u32::MAX.to_le_bytes());
    check(&huge, false);
    check(&b[..0x41a], false);
}
