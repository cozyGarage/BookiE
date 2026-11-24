use bookie_core::TableSize;

use super::*;

fn size(schema: Option<&str>, name: &str, bytes: u64) -> TableSize {
    TableSize {
        schema: schema.map(str::to_string),
        name: name.into(),
        bytes,
    }
}

#[test]
fn sizes_read_in_binary_units_with_one_decimal_below_ten() {
    assert_eq!(size_label(0), "0 B");
    assert_eq!(size_label(512), "512 B");
    assert_eq!(size_label(1536), "1.5 KiB");
    assert_eq!(size_label(12 * 1024 * 1024), "12 MiB");
    assert_eq!(size_label(3 * 1024 * 1024 * 1024 + 300 * 1024 * 1024), "3.3 GiB");
    assert_eq!(size_label(u64::MAX), "16384 PiB");
}

#[test]
fn a_size_is_found_by_schema_and_name_or_by_a_unique_name_when_the_sidebar_has_no_schema() {
    let index = SizeIndex::new(vec![
        size(Some("public"), "users", 100),
        size(Some("audit"), "users", 200),
        size(Some("shop"), "orders", 300),
    ]);
    assert_eq!(index.get(Some("audit"), "users"), Some(200));
    assert_eq!(
        index.get(None, "orders"),
        Some(300),
        "single-database engines list tables without a schema"
    );
    assert_eq!(index.get(None, "users"), None, "an ambiguous name has no size");
    assert_eq!(index.get(Some("public"), "missing"), None);
    assert!(SizeIndex::default().is_empty());
}
