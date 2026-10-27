use super::*;

#[test]
fn value_contract_bit_strings_keep_leading_zeroes_and_reject_malformed_wire_values() {
    assert_eq!(
        decode_bit_string(&[0, 0, 0, 10, 0b0010_1101, 0b1000_0000]).as_deref(),
        Some("0010110110")
    );
    assert_eq!(decode_bit_string(&[0, 0, 0, 0]), Some(String::new()));
    assert_eq!(decode_bit_string(&[0, 0, 0, 1, 0b1000_0001]), None);
    assert_eq!(decode_bit_string(&[0, 0, 0, 9, 0b1010_1010]), None);
    assert_eq!(decode_bit_string(&[0xff, 0xff, 0xff, 0xff]), None);
}

#[test]
fn value_contract_macaddr_requires_six_bytes_and_uses_postgres_lowercase_text() {
    assert_eq!(
        decode_pg_binary_text("MACADDR", &[0x08, 0x00, 0x2b, 0x01, 0x02, 0x03]).as_deref(),
        Some("08:00:2b:01:02:03")
    );
    assert_eq!(decode_macaddr(&[0x08, 0x00, 0x2b, 0x01, 0x02]), None);
    assert_eq!(decode_macaddr(&[0x08, 0x00, 0x2b, 0x01, 0x02, 0x03, 0x04]), None);
}

#[test]
fn value_contract_xml_is_utf8_text_and_oid_is_an_unsigned_integer() {
    assert_eq!(decode_pg_binary_text("XML", b"<a/>").as_deref(), Some("<a/>"));
    assert_eq!(decode_pg_binary_text("XML", &[0xff, 0xfe]), None);
    assert_eq!(
        decode_pg_binary_text("OID", &[0xff, 0xff, 0xff, 0xff]).as_deref(),
        Some("4294967295")
    );
    assert_eq!(decode_pg_binary_text("OID", &[0, 0, 5]), None);
}

#[test]
fn value_contract_macaddr8_requires_eight_bytes_and_uses_postgres_lowercase_text() {
    assert_eq!(
        decode_pg_binary_text("MACADDR8", &[0x08, 0x00, 0x2b, 0xff, 0xfe, 0x01, 0x02, 0x03]).as_deref(),
        Some("08:00:2b:ff:fe:01:02:03")
    );
    assert_eq!(decode_macaddr8(&[0x08, 0x00, 0x2b, 0xff, 0xfe, 0x01, 0x02]), None);
    assert_eq!(
        decode_macaddr8(&[0x08, 0x00, 0x2b, 0xff, 0xfe, 0x01, 0x02, 0x03, 0x04]),
        None
    );
}

#[test]
fn value_contract_extreme_intervals_and_malformed_network_values_do_not_panic() {
    for days in [i32::MIN, -1, 0, 1, i32::MAX] {
        for micros in [i64::MIN, -1, 0, 1, i64::MAX] {
            assert!(decode_interval(&interval_bytes(i32::MIN, days, micros)).is_some());
        }
    }
    for prefix in 33..=255 {
        assert_eq!(decode_inet(&[2, prefix, 0, 4, 192, 0, 2, 1], false), None);
    }
    for length in 0..64 {
        for byte in 0..=255 {
            for kind in ["INTERVAL", "INET", "CIDR", "PG_LSN"] {
                let _ = decode_pg_binary_text(kind, &vec![byte; length]);
            }
        }
    }
}

fn interval_bytes(months: i32, days: i32, microseconds: i64) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    bytes[0..8].copy_from_slice(&microseconds.to_be_bytes());
    bytes[8..12].copy_from_slice(&days.to_be_bytes());
    bytes[12..16].copy_from_slice(&months.to_be_bytes());
    bytes
}

#[test]
fn value_contract_interval_extremes_and_mixed_signs_are_unambiguous() {
    for (months, days, micros, expected) in [
        (-1, 2, 3, "-1 mons +2 days 00:00:00.000003"),
        (1, -2, 3, "1 mon -2 days +00:00:00.000003"),
        (1, 2, -3, "1 mon 2 days -00:00:00.000003"),
        (0, 0, i64::MIN, "-9223372036854.775808 seconds"),
        (0, 0, i64::MAX, "2562047788:00:54.775807"),
        (0, 0, i64::from(i32::MAX) * 3_600_000_000, "2147483647:00:00"),
    ] {
        assert_eq!(
            decode_interval(&interval_bytes(months, days, micros)).as_deref(),
            Some(expected)
        );
    }
    for length in 0..32 {
        assert_eq!(decode_interval(&vec![0; length]).is_some(), length == 16);
    }
}

#[test]
fn value_contract_interval_query_duration_matches_postgres_style() {
    assert_eq!(
        decode_pg_binary_text("INTERVAL", &interval_bytes(0, 0, 1_234_567)).as_deref(),
        Some("00:00:01.234567")
    );
    assert_eq!(
        decode_pg_binary_text("INTERVAL", &interval_bytes(0, 0, 3_723_000_000)).as_deref(),
        Some("01:02:03")
    );
    assert_eq!(
        decode_pg_binary_text("INTERVAL", &interval_bytes(0, 3, 0)).as_deref(),
        Some("3 days")
    );
    assert_eq!(
        decode_pg_binary_text(
            "INTERVAL",
            &interval_bytes(14, 3, 4 * 3_600_000_000 + 5 * 60_000_000 + 6_000_000)
        )
        .as_deref(),
        Some("1 year 2 mons 3 days 04:05:06")
    );
    assert_eq!(
        decode_pg_binary_text("INTERVAL", &interval_bytes(0, 0, 0)).as_deref(),
        Some("00:00:00")
    );
    assert_eq!(
        decode_pg_binary_text("INTERVAL", &interval_bytes(0, 0, -3_600_000_000)).as_deref(),
        Some("-01:00:00")
    );
}

#[test]
fn value_contract_inet_and_cidr_use_postgres_text() {
    let inet = [2, 32, 0, 4, 192, 0, 2, 1];
    assert_eq!(decode_pg_binary_text("INET", &inet).as_deref(), Some("192.0.2.1"));
    let cidr = [2, 24, 1, 4, 192, 0, 2, 0];
    assert_eq!(decode_pg_binary_text("CIDR", &cidr).as_deref(), Some("192.0.2.0/24"));
    let mut ipv6 = vec![3, 128, 0, 16];
    ipv6.extend_from_slice(&[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    assert_eq!(decode_pg_binary_text("INET", &ipv6).as_deref(), Some("2001:db8::1"));
}

#[test]
fn value_contract_pg_lsn_uses_uppercase_hex() {
    let bytes = 0x0000_0000_016B_3748_u64.to_be_bytes();
    assert_eq!(decode_pg_binary_text("PG_LSN", &bytes).as_deref(), Some("0/16B3748"));
}

#[test]
fn value_contract_unknown_binary_types_stay_undecoded() {
    assert_eq!(decode_pg_binary_text("INT4RANGE", &[1, 2, 3, 4]), None);
    assert_eq!(decode_pg_binary_text("INTERVAL", &[0, 1, 2]), None);
}

#[test]
fn value_contract_bit_wire_lengths_pin_byte_boundaries_and_padding() {
    for (length, payload, expected) in [
        (0_i32, vec![], ""),
        (1, vec![0x80], "1"),
        (7, vec![0xaa], "1010101"),
        (8, vec![0xa5], "10100101"),
        (9, vec![0xa5, 0x80], "101001011"),
        (16, vec![0xa5, 0x5a], "1010010101011010"),
        (17, vec![0xa5, 0x5a, 0x80], "10100101010110101"),
    ] {
        let mut bytes = length.to_be_bytes().to_vec();
        bytes.extend_from_slice(&payload);
        for kind in ["BIT", "VARBIT"] {
            assert_eq!(decode_pg_binary_text(kind, &bytes).as_deref(), Some(expected));
            let mut extra = bytes.clone();
            extra.push(0);
            assert_eq!(decode_pg_binary_text(kind, &extra), None);
            assert_eq!(decode_pg_binary_text(kind, &bytes[..bytes.len() - 1]), None);
        }
    }
    for length in 1..8 {
        for padding_bit in 0..(8 - length) {
            let bytes = [0, 0, 0, length, 0x80 | (1 << padding_bit)];
            assert_eq!(decode_pg_binary_text("VARBIT", &bytes), None);
        }
    }
    for length in 0..4 {
        assert_eq!(decode_pg_binary_text("BIT", &vec![0; length]), None);
    }
}

#[test]
fn value_contract_network_prefixes_families_flags_and_lengths_are_checked() {
    for (prefix, expected) in [(0, "192.0.2.1/0"), (31, "192.0.2.1/31"), (32, "192.0.2.1")] {
        let bytes = [2, prefix, 0, 4, 192, 0, 2, 1];
        assert_eq!(decode_pg_binary_text("INET", &bytes).as_deref(), Some(expected));
    }
    for prefix in [0, 127, 128] {
        let mut bytes = vec![3, prefix, 0, 16];
        bytes.extend_from_slice(&[0; 16]);
        let expected = if prefix == 128 {
            "::".into()
        } else {
            format!("::/{prefix}")
        };
        assert_eq!(decode_pg_binary_text("INET", &bytes), Some(expected));
        bytes[2] = 1;
        assert_eq!(decode_pg_binary_text("CIDR", &bytes), Some(format!("::/{prefix}")));
    }
    let mut ipv6 = vec![3, 129, 0, 16];
    ipv6.extend_from_slice(&[0; 16]);
    assert_eq!(decode_pg_binary_text("INET", &ipv6), None);
    for bytes in [
        vec![],
        vec![2, 32, 0],
        vec![2, 32, 0, 3, 1, 2, 3],
        vec![2, 32, 2, 4, 192, 0, 2, 1],
        vec![4, 32, 0, 4, 192, 0, 2, 1],
        vec![3, 32, 0, 4, 192, 0, 2, 1],
        vec![2, 32, 0, 4, 192, 0, 2, 1, 0],
    ] {
        assert_eq!(decode_pg_binary_text("INET", &bytes), None);
    }
}

#[test]
fn value_contract_lsn_preserves_both_halves_and_requires_eight_bytes() {
    for (value, expected) in [
        (0_u64, "0/0"),
        (u32::MAX as u64, "0/FFFFFFFF"),
        (1_u64 << 32, "1/0"),
        (u64::MAX, "FFFFFFFF/FFFFFFFF"),
    ] {
        assert_eq!(
            decode_pg_binary_text("PG_LSN", &value.to_be_bytes()).as_deref(),
            Some(expected)
        );
    }
    for length in 0..16 {
        assert_eq!(decode_pg_binary_text("PG_LSN", &vec![0; length]).is_some(), length == 8);
    }
}
#[test]
fn value_contract_tsvector_wire_values_use_postgres_text() {
    let vector = [
        2_u32.to_be_bytes().to_vec(),
        b"it's\0".to_vec(),
        2_u16.to_be_bytes().to_vec(),
        1_u16.to_be_bytes().to_vec(),
        (0x8000_u16 | 3).to_be_bytes().to_vec(),
        b"b\0".to_vec(),
        0_u16.to_be_bytes().to_vec(),
    ]
    .concat();
    assert_eq!(
        decode_pg_binary_text("TSVECTOR", &vector).as_deref(),
        Some("'it''s':1,3B 'b'")
    );
    assert_eq!(decode_tsvector(&vector[..vector.len() - 1]), None);
}
#[test]
fn value_contract_point_and_integer_range_wire_values_use_postgres_text() {
    let mut point = 1.5_f64.to_be_bytes().to_vec();
    point.extend(2.0_f64.to_be_bytes());
    assert_eq!(decode_pg_binary_text("POINT", &point).as_deref(), Some("(1.5,2)"));
    assert_eq!(decode_point(&point[..15]), None);

    let int4 = |value: i32| [&4_u32.to_be_bytes()[..], &value.to_be_bytes()].concat();
    let range = [vec![0x02], int4(1), int4(5)].concat();
    assert_eq!(decode_pg_binary_text("INT4RANGE", &range).as_deref(), Some("[1,5)"));
    assert_eq!(decode_int_range(&[0x01], 4).as_deref(), Some("empty"));
    let unbounded = [vec![0x02 | 0x10], int4(7)].concat();
    assert_eq!(decode_int_range(&unbounded, 4).as_deref(), Some("[7,)"));
    let int8 = |value: i64| [&8_u32.to_be_bytes()[..], &value.to_be_bytes()].concat();
    let wide = [vec![0x02 | 0x04], int8(i64::MAX), int8(i64::MAX)].concat();
    assert_eq!(
        decode_pg_binary_text("INT8RANGE", &wide).as_deref(),
        Some("[9223372036854775807,9223372036854775807]")
    );
    assert_eq!(decode_int_range(&range[..range.len() - 1], 4), None);
    assert_eq!(decode_int_range(&[range, vec![0]].concat(), 4), None);
}

fn all_short_inputs() -> Vec<Vec<u8>> {
    let alphabet = [0_u8, 1, 2, 4, 8, 0x10, 0x7f, 0x80, 0xff];
    let mut all = vec![Vec::new()];
    let mut frontier = vec![Vec::new()];
    for _ in 0..3 {
        frontier = frontier
            .iter()
            .flat_map(|prefix: &Vec<u8>| {
                alphabet.iter().map(move |byte| {
                    let mut next = prefix.clone();
                    next.push(*byte);
                    next
                })
            })
            .collect();
        all.extend(frontier.iter().cloned());
    }
    all
}

#[test]
fn malformed_wire_values_are_refused_and_never_panic() {
    let types = [
        "INTERVAL",
        "BIT",
        "VARBIT",
        "MACADDR",
        "MACADDR8",
        "INET",
        "CIDR",
        "PG_LSN",
        "XML",
        "OID",
        "XID",
        "CID",
        "POINT",
        "INT4RANGE",
        "INT8RANGE",
        "TSVECTOR",
        "NOT_A_TYPE",
    ];
    let mut random = crate::test_rng::Xorshift::new(0x9E37_79B9_7F4A_7C15);
    let mut inputs = all_short_inputs();
    inputs.extend((0..30_000).map(|_| random.bytes(48)));
    for bytes in &inputs {
        for type_name in types {
            let _ = decode_pg_binary_text(type_name, bytes);
        }
        let _ = crate::numeric::decode_binary(bytes);
        for type_name in ["DATE", "TIMESTAMP", "TIMESTAMPTZ"] {
            let _ = crate::temporal::decode_temporal(bytes, type_name);
        }
        for with_zone in [false, true] {
            let _ = crate::temporal::decode_time(bytes, with_zone);
        }
    }
}

#[test]
fn floats_switch_to_exponent_form_where_the_server_does() {
    for (value, expected) in [
        (1e14, "100000000000000"),
        (1e15, "1e+15"),
        (1.5e-4, "0.00015"),
        (1e-4, "0.0001"),
        (1e-5, "1e-05"),
        (1.5e21, "1.5e+21"),
        (5e-324, "5e-324"),
        (-2.5e-7, "-2.5e-07"),
        (0.0, "0"),
        (-0.0, "-0"),
        (123456789.5, "123456789.5"),
    ] {
        assert_eq!(float8_text(value), expected, "float8 {value:e}");
    }
    for (value, expected) in [
        (999999.0_f32, "999999"),
        (1e6, "1e+06"),
        (1.6777216e7, "1.6777216e+07"),
        (1.5e-5, "1.5e-05"),
        (0.1, "0.1"),
    ] {
        assert_eq!(float4_text(value), expected, "float4 {value:e}");
    }
    assert_eq!(float8_text(f64::NAN), "NaN");
    assert_eq!(float8_text(f64::INFINITY), "Infinity");
    assert_eq!(float4_text(f32::NEG_INFINITY), "-Infinity");
}
