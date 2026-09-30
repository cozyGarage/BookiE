use super::parse_input_for_driver;
use tablepro_core::{ColumnInfo, Value};

#[test]
fn postgres_float8_array_grid_literal_keeps_subnormal_and_signed_zero_text() {
    let literal = "{1.0000000000000002,-0,5e-324,NaN,Infinity,-Infinity,NULL}";
    let columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            primary_key: true,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
        },
        ColumnInfo {
            name: "value".into(),
            data_type: "float8[]".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
        },
    ];
    let parsed = parse_input_for_driver(literal, Some(&columns[1]), "postgres").unwrap();
    assert_eq!(parsed, Value::Text(literal.into()));
    let (sql, params) = tablepro_core::sql_dialect::build_keyed_update(
        "postgres",
        None,
        "float8_array_grid",
        &columns,
        &[(1, parsed)],
        &[Value::Int(1)],
    )
    .unwrap();
    assert!(sql.contains("$1::text::pg_catalog.float8[]"));
    assert_eq!(params[0], Value::Text(literal.into()));
}
