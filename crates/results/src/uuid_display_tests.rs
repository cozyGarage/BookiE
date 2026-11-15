use bookie_core::{ColumnInfo, Value};

use crate::uuid_display::format_binary_uuid;

fn column(data_type: &str) -> ColumnInfo {
    ColumnInfo {
        name: "id".into(),
        data_type: data_type.into(),
        nullable: false,
        primary_key: true,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

#[test]
fn binary16_values_paint_as_standard_uuid_text_without_changing_bytes() {
    let bytes = [
        0x55, 0x0e, 0x84, 0x00, 0xe2, 0x9b, 0x41, 0xd4, 0xa7, 0x16, 0x44, 0x66, 0x55, 0x44, 0x00, 0x00,
    ];
    let value = Value::Bytes(bytes.to_vec());

    assert_eq!(
        format_binary_uuid(&value, &column("binary(16)")),
        Some("550e8400-e29b-41d4-a716-446655440000".to_owned())
    );
    assert_eq!(value, Value::Bytes(bytes.to_vec()));
}

#[test]
fn mysql_query_result_binary_metadata_uses_the_same_uuid_paint() {
    let bytes = [
        0x55, 0x0e, 0x84, 0x00, 0xe2, 0x9b, 0x41, 0xd4, 0xa7, 0x16, 0x44, 0x66, 0x55, 0x44, 0x00, 0x00,
    ];
    let value = Value::Bytes(bytes.to_vec());

    assert_eq!(
        format_binary_uuid(&value, &column("VARBINARY")),
        Some("550e8400-e29b-41d4-a716-446655440000".to_owned())
    );
}

#[test]
fn uuid_paint_leaves_non_binary_or_non_sixteen_byte_values_unchanged() {
    assert_eq!(format_binary_uuid(&Value::Bytes(vec![0; 16]), &column("blob")), None);
    assert_eq!(
        format_binary_uuid(&Value::Bytes(vec![0; 15]), &column("binary(16)")),
        None
    );
    assert_eq!(
        format_binary_uuid(&Value::Bytes(vec![0; 16]), &column("binary(8)")),
        None
    );
    assert_eq!(
        format_binary_uuid(&Value::Text("not bytes".into()), &column("binary(16)")),
        None
    );
}
