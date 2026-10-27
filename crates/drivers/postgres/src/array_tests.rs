use super::*;

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|value| value.to_be_bytes()).collect()
}

#[test]
fn value_contract_vectors_render_space_separated_elements_and_refuse_other_headers() {
    let empty = words(&[1, 0, 21, 0, 0]);
    let pair = [words(&[1, 0, 21, 2, 0, 2]), vec![0, 1], words(&[2]), vec![0x80, 0]].concat();
    let oids = [words(&[1, 0, 26, 2, 0, 4, 0, 4]), vec![0xff; 4]].concat();
    assert_eq!(decode_vector(&empty, 21).as_deref(), Some(""));
    assert_eq!(decode_vector(&pair, 21).as_deref(), Some("1 -32768"));
    assert_eq!(decode_vector(&oids, 26).as_deref(), Some("0 4294967295"));
    assert_eq!(decode_vector(&pair, 26), None);
    for header in [
        [0, 0, 21, 0, 0],
        [2, 0, 21, 0, 0],
        [1, 1, 21, 0, 0],
        [1, 0, 21, 0, 1],
        [1, 0, 21, 1, 0],
    ] {
        assert_eq!(decode_vector(&words(&header), 21), None, "{header:?}");
    }
    assert_eq!(decode_vector(&[empty.clone(), vec![0]].concat(), 21), None);
}

#[test]
fn value_contract_vector_capacity_is_bounded_by_element_length_words() {
    assert_eq!(vector_capacity(2, 8), Some(2));
    assert_eq!(vector_capacity(3, 8), None);
    assert_eq!(vector_capacity(usize::MAX, 8), None);
}

#[test]
fn value_contract_temporal_array_elements_reject_malformed_payloads() {
    for (oid, length) in [(1082, 4), (1083, 8), (1114, 8), (1184, 8), (1186, 16), (1266, 12)] {
        for size in 0..24 {
            assert_eq!(
                temporal_element(oid, &vec![0; size]).is_some(),
                size == length,
                "{oid}: {size}"
            );
        }
        assert_eq!(decode_binary(&wire(oid, &[], &[]), oid, false).as_deref(), Some("{}"));
        assert_eq!(
            decode_binary(&wire(oid, &[(1, 1)], &[None]), oid, false).as_deref(),
            Some("{NULL}")
        );
    }
    assert_eq!(temporal_element(9999, &[]), None);
    assert_eq!(temporal_element(1083, &(-1_i64).to_be_bytes()), None);
    assert_eq!(temporal_element(1082, &(i32::MIN + 1).to_be_bytes()), None);
    assert_eq!(temporal_element(1082, &(i32::MAX - 1).to_be_bytes()), None);
    assert_eq!(temporal_element(1114, &(i64::MAX - 1).to_be_bytes()), None);
    assert_eq!(temporal_element(1184, &(i64::MIN + 1).to_be_bytes()), None);
    for (value, text) in [(i32::MIN, "-infinity"), (i32::MAX, "infinity")] {
        assert_eq!(temporal_element(1082, &value.to_be_bytes()).as_deref(), Some(text));
    }
    for oid in [1114, 1184] {
        for (value, text) in [(i64::MIN, "-infinity"), (i64::MAX, "infinity")] {
            assert_eq!(temporal_element(oid, &value.to_be_bytes()).as_deref(), Some(text));
        }
    }
}

fn wire(oid: u32, dimensions: &[(i32, i32)], elements: &[Option<&[u8]>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for word in [dimensions.len() as i32, i32::from(elements.contains(&None)), oid as i32] {
        bytes.extend_from_slice(&word.to_be_bytes());
    }
    for (length, lower) in dimensions {
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(&lower.to_be_bytes());
    }
    for element in elements {
        bytes.extend_from_slice(&element.map_or(-1, |value| value.len() as i32).to_be_bytes());
        if let Some(value) = element {
            bytes.extend_from_slice(value);
        }
    }
    bytes
}

#[test]
fn value_contract_bit_arrays_decode_binary_elements_and_refuse_nul_text() {
    for (oid, value, expected) in [
        (BIT_OID, &[0, 0, 0, 5, 0b1010_0000][..], "{\"10100\"}"),
        (VARBIT_OID, &[0, 0, 0, 3, 0b1010_0000][..], "{\"101\"}"),
    ] {
        assert_eq!(
            decode_binary(&wire(oid, &[(1, 1)], &[Some(value)]), oid, false).as_deref(),
            Some(expected)
        );
    }
    for oid in [19, 25, 1042, 1043] {
        assert_eq!(
            decode_binary(&wire(oid, &[(1, 1)], &[Some(b"a\0b")]), oid, false),
            None,
            "OID {oid}"
        );
    }
}

#[test]
fn value_contract_array_null_empty_and_escaped_text_stay_distinct() {
    assert_eq!(decode_binary(&wire(25, &[], &[]), 25, false).as_deref(), Some("{}"));
    let bytes = wire(
        25,
        &[(5, 1)],
        &[None, Some(b"NULL"), Some(b""), Some(b"a\\\"b"), Some("漢字".as_bytes())],
    );
    assert_eq!(
        decode_binary(&bytes, 25, false).as_deref(),
        Some(r#"{NULL,"NULL","","a\\\"b","漢字"}"#)
    );
    let integer = 1_i32.to_be_bytes();
    let bytes = wire(
        23,
        &[(2, 0), (2, -1)],
        &[Some(&integer), None, Some(&integer), Some(&integer)],
    );
    assert_eq!(
        decode_binary(&bytes, 23, false).as_deref(),
        Some(r#"[0:1][-1:0]={{"1",NULL},{"1","1"}}"#)
    );
}

#[test]
fn value_contract_enum_array_labels_quote_null_text_and_preserve_sql_null() {
    let bytes = wire(
        9001,
        &[(6, 1)],
        &[
            Some(b"NULL"),
            Some(b""),
            Some(b"a,b"),
            Some(b"a\"b"),
            Some("東京".as_bytes()),
            None,
        ],
    );

    assert_eq!(
        decode_binary(&bytes, 9001, true).as_deref(),
        Some(r#"{"NULL","","a,b","a\"b","東京",NULL}"#)
    );
    assert_eq!(element_text(9001, true, &[0xff]), None);
}

#[test]
fn value_contract_pg_lsn_arrays_decode_maximum_and_null_elements() {
    let zero = 0_u64.to_be_bytes();
    let low_max = u64::from(u32::MAX).to_be_bytes();
    let high_one = (1_u64 << 32).to_be_bytes();
    let maximum = u64::MAX.to_be_bytes();
    let bytes = wire(
        PG_LSN_OID,
        &[(5, 0)],
        &[Some(&zero), Some(&low_max), Some(&high_one), Some(&maximum), None],
    );
    assert_eq!(
        decode_binary(&bytes, PG_LSN_OID, false).as_deref(),
        Some("[0:4]={\"0/0\",\"0/FFFFFFFF\",\"1/0\",\"FFFFFFFF/FFFFFFFF\",NULL}")
    );
}

#[test]
fn value_contract_macaddr_arrays_decode_binary_elements_and_sql_null() {
    let mac = [0x08, 0x00, 0x2b, 0x01, 0x02, 0x03];
    let bytes = wire(MACADDR_OID, &[(2, 0)], &[Some(&mac), None]);
    assert_eq!(
        decode_binary(&bytes, MACADDR_OID, false).as_deref(),
        Some(r#"[0:1]={"08:00:2b:01:02:03",NULL}"#)
    );
    assert_eq!(element_text(MACADDR_OID, false, &[0; 5]), None);
}

#[test]
fn value_contract_macaddr8_arrays_decode_eight_octets_null_and_bounds() {
    let mac = [0x08, 0x00, 0x2b, 0x01, 0x02, 0x03, 0x04, 0x05];
    let bytes = wire(MACADDR8_OID, &[(2, 0)], &[Some(&mac), None]);
    assert_eq!(
        decode_binary(&bytes, MACADDR8_OID, false).as_deref(),
        Some(r#"[0:1]={"08:00:2b:01:02:03:04:05",NULL}"#)
    );
    assert_eq!(element_text(MACADDR8_OID, false, &[0; 7]), None);
}

#[test]
fn value_contract_inet_and_cidr_arrays_decode_network_bytes() {
    let ipv4_inet = [2, 24, 0, 4, 192, 0, 2, 1];
    let ipv6_inet = [3, 64, 0, 16, 0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
    let inet = wire(INET_OID, &[(2, 0)], &[Some(&ipv4_inet), Some(&ipv6_inet)]);
    assert_eq!(
        decode_binary(&inet, INET_OID, false).as_deref(),
        Some(r#"[0:1]={"192.0.2.1/24","2001:db8::1/64"}"#)
    );

    let ipv4_cidr = [2, 24, 1, 4, 192, 0, 2, 0];
    let ipv6_cidr = [3, 32, 1, 16, 0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let cidr = wire(CIDR_OID, &[(2, -1)], &[Some(&ipv4_cidr), Some(&ipv6_cidr)]);
    assert_eq!(
        decode_binary(&cidr, CIDR_OID, false).as_deref(),
        Some(r#"[-1:0]={"192.0.2.0/24","2001:db8::/32"}"#)
    );
}

#[test]
fn value_contract_domain_array_decodes_elements_by_base_type_oid() {
    let uuid = uuid::Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
    let bytes = wire(9002, &[(2, 1)], &[Some(uuid.as_bytes()), None]);

    assert_eq!(
        decode_binary_with_oid(&bytes, 9002, 2950, false).as_deref(),
        Some(r#"{"123e4567-e89b-12d3-a456-426614174000",NULL}"#)
    );
    assert_eq!(decode_binary_with_oid(&bytes, 9003, 2950, false), None);
}

#[test]
fn value_contract_malformed_array_dimensions_lengths_types_and_elements_are_refused() {
    let valid = wire(25, &[(1, 1)], &[Some(b"value")]);
    for length in 0..valid.len() {
        assert_eq!(decode_binary(&valid[..length], 25, false), None);
    }
    assert_eq!(decode_binary(&valid, 23, false), None);
    for (offset, word) in [
        (0, -1_i32),
        (0, 7),
        (4, 2),
        (12, 0),
        (12, i32::MAX),
        (20, -2),
        (20, i32::MAX),
    ] {
        let mut bytes = valid.clone();
        bytes[offset..offset + 4].copy_from_slice(&word.to_be_bytes());
        assert_eq!(decode_binary(&bytes, 25, false), None, "{offset}: {word}");
    }
    let mut trailing = valid;
    trailing.push(0);
    assert_eq!(decode_binary(&trailing, 25, false), None);
    let mut forbidden_null = wire(25, &[(1, 1)], &[None]);
    forbidden_null[4..8].copy_from_slice(&0_i32.to_be_bytes());
    assert_eq!(decode_binary(&forbidden_null, 25, false), None);
    for bytes in [
        wire(25, &[(2, i32::MAX)], &[None, None]),
        wire(25, &[(i32::MAX, 1); 6], &[]),
        wire(25, &[(1, 1)], &[Some(&[255])]),
        wire(25, &[(1, 1)], &[Some(b"a\0b")]),
    ] {
        assert_eq!(decode_binary(&bytes, 25, false), None);
    }
    assert_eq!(decode_binary(&wire(9999, &[], &[]), 9999, false), None);
    assert_eq!(element_text(16, false, &[2]), None);
    assert_eq!(element_text(23, false, &[1, 2]), None);
}

#[test]
fn value_contract_array_header_mutations_and_maximum_depth_are_bounded() {
    let bytes = wire(25, &[(1, i32::MIN); 6], &[Some(b"value")]);
    assert!(decode_binary(&bytes, 25, false).is_some());
    for offset in 0..bytes.len() {
        for replacement in [0, 1, 127, 128, 254, 255] {
            let mut mutated = bytes.clone();
            mutated[offset] = replacement;
            let _ = decode_binary(&mutated, 25, false);
        }
    }
    let mut state = 12345_u32;
    for length in 0..128 {
        let mut bytes = vec![0; length];
        for byte in &mut bytes {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            *byte = (state >> 24) as u8;
        }
        let _ = decode_binary(&bytes, 25, false);
    }
}

#[test]
fn value_contract_expanded_array_text_has_an_explicit_size_bound() {
    assert_eq!(MAX_ARRAY_TEXT_BYTES, 16_777_216);
    let mut output = String::new();
    assert!(append_quoted(&mut output, &"x".repeat(MAX_ARRAY_TEXT_BYTES)).is_none());
    assert!(output.is_empty());
    assert!(append_quoted(&mut output, &"\\".repeat(MAX_ARRAY_TEXT_BYTES / 2)).is_none());
    assert!(output.is_empty());
    assert_eq!(append_quoted(&mut output, &"x".repeat(16_777_214)), Some(()));
    assert_eq!(output.len(), 16_777_216);
}

#[test]
fn value_contract_array_wire_limits_are_checked_before_decoding() {
    let payload = vec![b'x'; 16_777_217];
    let mut reader = Reader { remaining: &payload };
    assert_eq!(read_bounded_element(&mut reader, 16_777_217), None);
    assert_eq!(reader.remaining.len(), payload.len());
    assert_eq!(read_bounded_element(&mut reader, 16_777_216).unwrap().len(), 16_777_216);
    assert_eq!(reader.remaining.len(), 1);
    assert_eq!(read_bounded_element(&mut reader, -1), None);
    let bytes = wire(25, &[(2, 1)], &[Some(b"")]);
    let mut reader = Reader {
        remaining: &bytes[12..],
    };
    assert!(read_dimensions(&mut reader, 1).is_none());
}

#[test]
fn value_contract_dimension_product_must_fit_element_length_words() {
    for (payload_bytes, expected) in [(7, false), (8, true)] {
        let mut bytes = words(&[2, 1]);
        bytes.extend(vec![0; payload_bytes]);
        let mut reader = Reader { remaining: &bytes };
        assert_eq!(read_dimensions(&mut reader, 1).is_some(), expected, "{payload_bytes}");
    }
}
#[test]
fn json_and_jsonb_elements_decode_to_their_text_and_refuse_other_versions() {
    assert_eq!(
        element_text(114, false, br#"{"a":   1}"#).as_deref(),
        Some(r#"{"a":   1}"#)
    );
    assert_eq!(element_text(3802, false, b"\x01[1, 2]").as_deref(), Some("[1, 2]"));
    assert_eq!(element_text(3802, false, b"\x02[1, 2]"), None);
    assert_eq!(element_text(3802, false, b""), None);
    assert!(supported(114) && supported(3802));
}
fn int4_array(values: &[i32]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for field in [1_i32, 0, 23, values.len() as i32, 1] {
        bytes.extend(field.to_be_bytes());
    }
    for value in values {
        bytes.extend(4_i32.to_be_bytes());
        bytes.extend(value.to_be_bytes());
    }
    bytes
}

#[test]
fn mutated_array_wire_values_are_refused_and_never_panic() {
    let samples = [
        int4_array(&[1, 2, 3]),
        int4_array(&[]),
        int4_array(&[i32::MIN, i32::MAX]),
    ];
    let types = [
        (23_u32, 23_u32),
        (25, 25),
        (114, 114),
        (3802, 3802),
        (1700, 1700),
        (701, 701),
        (700, 700),
        (1184, 1184),
        (2950, 2950),
        (17, 17),
        (1082, 1082),
    ];
    let mut random = crate::test_rng::Xorshift::new(0xD1B5_4A32_D192_ED03);
    let mut next = move || random.next();
    for _ in 0..40_000 {
        let mut bytes = samples[(next() % 3) as usize].clone();
        for _ in 0..(next() % 4) {
            match next() % 4 {
                0 if !bytes.is_empty() => {
                    let at = (next() as usize) % bytes.len();
                    bytes[at] = (next() >> 8) as u8;
                }
                1 if !bytes.is_empty() => bytes.truncate((next() as usize) % bytes.len()),
                2 => bytes.push((next() >> 8) as u8),
                _ if bytes.len() >= 4 => {
                    let at = (next() as usize) % (bytes.len() - 3);
                    bytes[at..at + 4].copy_from_slice(&[0x7f, 0xff, 0xff, 0xff]);
                }
                _ => {}
            }
        }
        for (expected, value) in types {
            for text_element in [false, true] {
                let _ = decode_binary_with_oid(&bytes, expected, value, text_element);
            }
        }
    }
}
