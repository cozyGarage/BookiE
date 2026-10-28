use super::{encode_pg_lsn, encode_pg_mac_address, encode_pg_network, parse_text_array_text};
use tablepro_core::DriverError;

#[test]
fn inferred_text_array_text_rejects_nesting_beyond_postgres_dimension_limit() {
    assert!(parse_text_array_text("{{{{{{ready}}}}}}").is_ok());

    let deeply_nested = format!("{}ready{}", "{".repeat(10_000), "}".repeat(10_000));

    assert!(matches!(
        parse_text_array_text(&deeply_nested),
        Err(DriverError::Unsupported(message)) if message.contains("at most six dimensions")
    ));
}

#[test]
fn inferred_text_array_text_preserves_quotes_escapes_nulls_and_bounds() {
    let parsed = parse_text_array_text(r#"[0:5]={"NULL",NULL,"","a,b","a\"b","a\\b"}"#).unwrap();
    assert_eq!(parsed.dimensions, vec![(6, 0)]);
    assert_eq!(
        parsed.elements,
        vec![
            Some("NULL".into()),
            None,
            Some(String::new()),
            Some("a,b".into()),
            Some("a\"b".into()),
            Some("a\\b".into()),
        ]
    );

    let parsed = parse_text_array_text(r#"[-1:0][3:4]={{ready,NULL},{"",東京}}"#).unwrap();
    assert_eq!(parsed.dimensions, vec![(2, -1), (2, 3)]);
    assert_eq!(
        parsed.elements,
        vec![Some("ready".into()), None, Some(String::new()), Some("東京".into()),]
    );
}

#[test]
fn inferred_text_array_text_refuses_malformed_shapes_and_trailing_data() {
    for text in [
        "{{ready},{paused,ready}}",
        "[0:2]={ready,paused}",
        "{} trailing",
        r#"{"unterminated\}"#,
        "{ready,paused]",
    ] {
        assert!(
            matches!(parse_text_array_text(text), Err(DriverError::Unsupported(_))),
            "malformed array text should be refused: {text:?}"
        );
    }
}

#[test]
fn value_contract_pg_lsn_array_elements_encode_full_unsigned_words() {
    for (text, expected) in [
        ("0/0", 0_u64),
        ("0/FFFFFFFF", u32::MAX as u64),
        ("1/0", 1_u64 << 32),
        ("FFFFFFFF/FFFFFFFF", u64::MAX),
    ] {
        assert_eq!(encode_pg_lsn(text), Some(expected.to_be_bytes()), "{text}");
    }
    for malformed in ["", "0", "/1", "1/", "1/2/3", "100000000/0", "0/100000000", "x/y"] {
        assert_eq!(encode_pg_lsn(malformed), None, "{malformed}");
    }
}

#[test]
fn value_contract_macaddr_array_elements_encode_six_octets_and_refuse_malformed_text() {
    assert_eq!(
        encode_pg_mac_address::<6>("08:00:2b:01:02:03"),
        Some([0x08, 0x00, 0x2b, 0x01, 0x02, 0x03])
    );
    assert_eq!(
        encode_pg_mac_address::<6>("AA:bb:CC:dd:EE:fF"),
        Some([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff])
    );
    for malformed in [
        "",
        "08:00:2b:01:02",
        "08:00:2b:01:02:03:04",
        "8:00:2b:01:02:03",
        "gg:00:2b:01:02:03",
    ] {
        assert_eq!(encode_pg_mac_address::<6>(malformed), None, "{malformed}");
    }
}

#[test]
fn value_contract_macaddr8_array_elements_encode_eight_octets_and_refuse_malformed_text() {
    assert_eq!(
        encode_pg_mac_address::<8>("08:00:2b:01:02:03:04:05"),
        Some([0x08, 0x00, 0x2b, 0x01, 0x02, 0x03, 0x04, 0x05])
    );
    for malformed in [
        "08:00:2b:01:02:03",
        "08:00:2b:01:02:03:04:0g",
        "08:00:2b:01:02:03:04:005",
        "08:00:2b:01:02:03:04:05:06",
    ] {
        assert_eq!(encode_pg_mac_address::<8>(malformed), None, "{malformed}");
    }
}

#[test]
fn value_contract_inet_and_cidr_array_elements_encode_network_byte_order_and_refuse_lossy_values() {
    assert_eq!(
        encode_pg_network("192.0.2.1/24", false),
        Some(vec![2, 24, 0, 4, 192, 0, 2, 1])
    );
    assert_eq!(
        encode_pg_network("192.0.2.1", false),
        Some(vec![2, 32, 0, 4, 192, 0, 2, 1])
    );
    assert_eq!(
        encode_pg_network("2001:db8::1/64", false),
        Some(vec![
            3, 64, 0, 16, 0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1
        ])
    );
    assert_eq!(
        encode_pg_network("192.0.2.0/24", true),
        Some(vec![2, 24, 1, 4, 192, 0, 2, 0])
    );
    assert_eq!(
        encode_pg_network("2001:db8::/32", true),
        Some(vec![
            3, 32, 1, 16, 0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ])
    );
    for malformed in ["", "192.0.2.1/33", "192.0.2.1/24/2", "not-an-address"] {
        assert_eq!(encode_pg_network(malformed, false), None, "inet: {malformed}");
    }
    for malformed in ["192.0.2.1/24", "192.0.2.0", "2001:db8::1/64", "2001:db8::/129"] {
        assert_eq!(encode_pg_network(malformed, true), None, "cidr: {malformed}");
    }
}

#[test]
fn text_array_parsing_rejects_ragged_escaped_and_malformed_input() {
    for (text, fragment) in [
        ("{{1},2}", "array"),
        ("{{1,2},{3}}", ""),
        ("{a\\", "escape"),
        ("[1:x]={a}", ""),
        ("[1:2]{a,b}", ""),
        ("[3:1]={a}", ""),
        ("[1:3]={a,b}", "do not match"),
        ("[1:1][1:1][1:1][1:1][1:1][1:1][1:1]={{{{{{{a}}}}}}}", "six dimensions"),
        ("abc", ""),
    ] {
        match parse_text_array_text(text) {
            Err(DriverError::Unsupported(message)) => {
                assert!(message.contains(fragment), "{text}: {message}");
            }
            other => panic!(
                "{text}: expected an unsupported error, got {:?}",
                other.map(|parsed| parsed.elements)
            ),
        }
    }
}

#[test]
fn text_array_parsing_keeps_empty_and_multidimensional_shapes() {
    let empty = parse_text_array_text("{}").unwrap();
    assert!(empty.elements.is_empty());
    let grid = parse_text_array_text("{{1,2},{3,4}}").unwrap();
    assert_eq!(grid.dimensions, vec![(2, 1), (2, 1)]);
    assert_eq!(grid.elements.len(), 4);
}

#[test]
fn every_value_kind_is_described_to_the_server_with_its_own_type() {
    use super::pg_parameter_type_infos;
    use sqlx::TypeInfo;
    use tablepro_core::Value;
    let values = [
        Value::Null,
        Value::Bool(true),
        Value::Int(1),
        Value::Float(1.5),
        Value::Text("a".into()),
        Value::JsonText("{}".into()),
        Value::Bytes(vec![1]),
        Value::Date(chrono::NaiveDate::from_ymd_opt(2026, 1, 2).unwrap()),
        Value::Time(chrono::NaiveTime::from_hms_opt(1, 2, 3).unwrap()),
        Value::DateTime(chrono::NaiveDateTime::default()),
        Value::TimestampTz(chrono::DateTime::<chrono::Utc>::default()),
        Value::Decimal(rust_decimal::Decimal::new(15, 1)),
        Value::Uuid(uuid::Uuid::nil()),
        Value::Json(Box::new(serde_json::json!({"a": 1}))),
        Value::Undecodable("MONEY".into()),
    ];
    let names: Vec<String> = pg_parameter_type_infos(&values)
        .iter()
        .map(|type_info| type_info.name().to_ascii_uppercase())
        .collect();
    assert_eq!(
        names,
        [
            "TEXT",
            "BOOL",
            "INT8",
            "FLOAT8",
            "TEXT",
            "JSON",
            "BYTEA",
            "DATE",
            "TIME",
            "TIMESTAMP",
            "TIMESTAMPTZ",
            "NUMERIC",
            "UUID",
            "JSONB",
            "TEXT"
        ]
    );
}

#[test]
fn the_untyped_null_and_json_text_markers_accept_the_right_server_types() {
    use super::{PgJsonText, PgUntypedNull};
    use sqlx::postgres::PgTypeInfo;
    use sqlx::{Encode, Type};
    assert!(<PgUntypedNull as Type<sqlx::Postgres>>::compatible(
        &PgTypeInfo::with_name("INT4")
    ));
    assert!(<PgJsonText as Type<sqlx::Postgres>>::compatible(
        &PgTypeInfo::with_name("json")
    ));
    assert!(!<PgJsonText as Type<sqlx::Postgres>>::compatible(
        &PgTypeInfo::with_name("jsonb")
    ));
    let mut buffer = sqlx::postgres::PgArgumentBuffer::default();
    assert!(matches!(
        Encode::<sqlx::Postgres>::encode_by_ref(&PgUntypedNull, &mut buffer),
        Ok(sqlx::encode::IsNull::Yes)
    ));
}
#[test]
fn arbitrary_text_is_parsed_or_refused_and_never_panics() {
    let alphabet = [
        '{', '}', ',', '"', '\\', 'a', ' ', 'N', 'U', 'L', '[', ']', ':', '=', '1', '\n', 'é',
    ];
    let mut random = crate::test_rng::Xorshift::new(0xA076_1D64_78BD_642F);
    let mut next = move || random.next();
    for _ in 0..60_000 {
        let length = next() % 24;
        let text: String = (0..length)
            .map(|_| alphabet[(next() as usize >> 4) % alphabet.len()])
            .collect();
        let _ = parse_text_array_text(&text);
    }
    let nested = format!("{}1{}", "{".repeat(5_000), "}".repeat(5_000));
    assert!(
        parse_text_array_text(&nested).is_err(),
        "deep nesting is refused, not recursed into"
    );
}
