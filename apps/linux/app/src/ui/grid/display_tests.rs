use super::*;

fn metadata(data_type: &str) -> ColumnInfo {
    ColumnInfo {
        name: "value".into(),
        data_type: data_type.into(),
        nullable: true,
        primary_key: false,
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
fn display_text_primitive_variants() {
    assert_eq!(value_to_display_text(&Value::Null), "NULL");
    assert_eq!(value_to_display_text(&Value::Bool(true)), "true");
    assert_eq!(value_to_display_text(&Value::Int(42)), "42");
    assert_eq!(value_to_display_text(&Value::Text("hello".into())), "hello");
    assert_eq!(value_to_display_text(&Value::Bytes(vec![0u8; 16])), "<16 bytes>");
}

#[test]
fn display_text_shows_valid_utf8_bytes_as_text() {
    assert_eq!(
        value_to_display_text(&Value::Bytes(b"hello world".to_vec())),
        "hello world"
    );
}

#[test]
fn display_text_keeps_invalid_utf8_bytes_as_a_byte_count() {
    assert_eq!(
        value_to_display_text(&Value::Bytes(vec![0xFF, 0xFE, 0x00, 0x01])),
        "<4 bytes>"
    );
}

#[test]
fn display_text_keeps_bytes_with_an_embedded_nul_as_a_byte_count() {
    assert_eq!(value_to_display_text(&Value::Bytes(b"ab\0cd".to_vec())), "<5 bytes>");
}

#[test]
fn display_text_shows_empty_bytes_as_empty_text() {
    assert_eq!(value_to_display_text(&Value::Bytes(Vec::new())), "");
}

#[test]
fn text_with_a_nul_shows_it_and_cannot_be_edited_inline() {
    let value = Value::Text("a\0b".into());
    assert_eq!(value_to_display_text(&value), "a\u{2400}b");
    assert!(!value_is_inline_editable(&value));
    assert_eq!(
        value_to_full_edit_text(&value),
        "a\0b",
        "an exact copy keeps the real NUL"
    );
}

#[test]
fn binary_bytes_are_not_inline_editable() {
    assert!(!value_is_inline_editable(&Value::Bytes(vec![0xFF, 0xFE, 0x00, 0x01])));
    assert!(!value_is_inline_editable(&Value::Bytes(b"hello world".to_vec())));
    assert!(!value_is_inline_editable(&Value::Bytes(Vec::new())));
    assert!(value_is_inline_editable(&Value::Text("hello".into())));
    assert!(value_is_inline_editable(&Value::Null));
    assert!(value_is_inline_editable(&Value::Int(1)));
}

#[test]
fn an_undecodable_cell_is_shown_distinctly_from_null_and_is_not_editable() {
    let value = Value::Undecodable("NUMERIC".into());
    assert_eq!(value_to_display_text(&value), "<undecodable NUMERIC>");
    assert_ne!(value_to_display_text(&value), value_to_display_text(&Value::Null));
    assert!(!value_is_inline_editable(&value));
}

#[test]
fn a_field_a_document_lacks_is_shown_as_missing_and_not_as_a_decoding_failure() {
    let missing = Value::Undecodable(Value::MISSING_FIELD.into());
    let shown = value_to_display_text(&missing);
    assert_eq!(shown, "(missing)");
    assert!(!shown.contains("undecodable"));
    assert_ne!(shown, value_to_display_text(&Value::Null));
    assert_ne!(
        cell_view(&missing, &metadata("mixed")).text_for_bind(true),
        editable_null_sentinel()
    );
    assert!(!value_is_inline_editable(&missing));
}

#[test]
fn bind_text_for_an_undecodable_cell_in_an_editable_column_stays_display_text() {
    let value = Value::Undecodable("NUMERIC".into());
    let view = cell_view(&value, &metadata("NUMERIC"));
    assert!(!view.inline_editable);
    assert_eq!(view.text_for_bind(true), "<undecodable NUMERIC>");
    assert_ne!(view.text_for_bind(true), editable_null_sentinel());
}

#[test]
fn the_null_display_style_changes_the_marker_and_never_the_meaning() {
    for style in NullDisplayPref::ALL {
        set_null_display(style);
        let marker = readonly_null_sentinel();
        assert_eq!(value_to_display_text(&Value::Null), marker);
        let null = cell_view(&Value::Null, &metadata("text"));
        assert!(null.is_null);
        assert_eq!(null.text_for_bind(true), editable_null_sentinel());
        let text = cell_view(&Value::Text(marker.clone()), &metadata("text"));
        assert!(!text.is_null);
        assert_eq!(text.text_for_bind(true), marker);
    }
    set_null_display(NullDisplayPref::Symbol);
    assert_eq!(readonly_null_sentinel(), "\u{2205}");
    set_null_display(NullDisplayPref::Parenthesized);
    assert_eq!(readonly_null_sentinel(), "(null)");
}

#[test]
fn a_text_value_that_looks_like_the_null_marker_is_text_and_not_null() {
    let text = Value::Text(editable_null_sentinel());
    let view = cell_view(&text, &metadata("text"));
    assert!(!view.is_null);
    assert!(view.inline_editable);
    assert_eq!(view.text_for_bind(true), editable_null_sentinel());
    assert!(cell_view(&Value::Null, &metadata("text")).is_null);
}

#[test]
fn bind_text_for_bytes_in_an_editable_column_stays_display_text() {
    let binary = Value::Bytes(vec![0xFF, 0xFE, 0x00, 0x01]);
    assert_eq!(cell_view(&binary, &metadata("text")).text_for_bind(true), "<4 bytes>");
    let utf8 = Value::Bytes(b"hello world".to_vec());
    assert_eq!(cell_view(&utf8, &metadata("text")).text_for_bind(true), "hello world");
    assert_eq!(
        cell_view(&Value::Text("hello".into()), &metadata("text")).text_for_bind(true),
        "hello"
    );
}

#[test]
fn cell_view_keeps_null_distinct_from_text_and_obeys_column_editability() {
    let text_column = metadata("text");
    let null = cell_view(&Value::Null, &text_column);
    assert!(null.is_null);
    assert!(null.inline_editable);
    assert_eq!(null.text_for_bind(true), "<NULL>");
    assert_eq!(null.text_for_bind(false), "NULL");
    let text_null = cell_view(&Value::Text("NULL".into()), &text_column);
    assert!(!text_null.is_null);
    assert_eq!(text_null.text_for_bind(true), "NULL");

    let mut protected_column = text_column;
    protected_column.primary_key = true;
    assert!(!cell_view(&Value::Null, &protected_column).inline_editable);
}

#[test]
fn cell_view_keeps_server_generated_null_and_preview_display_semantics() {
    let mut generated_column = metadata("text");
    generated_column.is_generated = true;
    let generated = cell_view(&Value::Null, &generated_column);
    assert_eq!(generated.text_for_bind(true), "(auto)");
    assert_eq!(generated.text_for_bind(false), "(auto)");

    let preview = crate::ui::row_object::CellPreview {
        value: Value::Text("sample".into()),
        byte_count: 9000,
    };
    let view = CellView::from_preview(&preview);
    assert!(!view.inline_editable);
    assert_eq!(view.text_for_bind(true), "sample… (9000 bytes total)");
}

#[test]
fn cell_view_maps_only_boolean_and_bit_one_values_to_checkbox_states() {
    let boolean = metadata("boolean");
    assert_eq!(cell_view(&Value::Bool(true), &boolean).checked, Some(true));
    assert_eq!(cell_view(&Value::Bool(false), &boolean).checked, Some(false));
    assert_eq!(cell_view(&Value::Int(1), &boolean).checked, None);

    let bit = metadata("BIT(1)");
    assert_eq!(cell_view(&Value::Int(1), &bit).checked, Some(true));
    assert_eq!(cell_view(&Value::Int(0), &bit).checked, Some(false));
    assert_eq!(cell_view(&Value::Int(2), &bit).checked, None);
    assert_eq!(cell_view(&Value::Null, &bit).checked, None);
}

#[test]
fn display_text_temporal_variants() {
    let date = chrono::NaiveDate::from_ymd_opt(2026, 4, 26).unwrap();
    assert_eq!(value_to_display_text(&Value::Date(date)), "2026-04-26");

    let time = chrono::NaiveTime::from_hms_opt(14, 30, 0).unwrap();
    assert_eq!(value_to_display_text(&Value::Time(time)), "14:30:00");

    let datetime = chrono::NaiveDateTime::new(date, time);
    assert_eq!(value_to_display_text(&Value::DateTime(datetime)), "2026-04-26 14:30:00");
    let fractional = datetime + chrono::Duration::milliseconds(10);
    assert_eq!(
        value_to_display_text(&Value::DateTime(fractional)),
        "2026-04-26 14:30:00.010"
    );

    let tz = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(datetime, chrono::Utc);
    assert_eq!(
        value_to_display_text(&Value::TimestampTz(tz)),
        "2026-04-26 14:30:00+00:00"
    );
}

#[test]
fn display_text_extended_variants() {
    let dec: rust_decimal::Decimal = "1234.56789".parse().unwrap();
    assert_eq!(value_to_display_text(&Value::Decimal(dec)), "1234.56789");

    let id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    assert_eq!(
        value_to_display_text(&Value::Uuid(id)),
        "550e8400-e29b-41d4-a716-446655440000"
    );

    let json = serde_json::json!({"a": 1, "b": [2, 3]});
    let text = value_to_display_text(&Value::Json(Box::new(json)));
    assert!(text.contains("\"a\":1"));
}

#[test]
fn truncate_short_text_passes_through() {
    let s = "hello world";
    assert_eq!(truncate_for_display(s), "hello world");
}

#[test]
fn truncate_caps_long_text_at_char_boundary() {
    let s = "a".repeat(100_000);
    let out = truncate_for_display(&s);
    assert!(out.starts_with(&"a".repeat(10_000)));
    assert!(out.ends_with("(+90000 more bytes)"));
    assert!(out.len() < 10_500);
}

#[test]
fn the_truncation_limits_are_forty_thousand_bytes_and_ten_thousand_chars() {
    assert_eq!(DISPLAY_TEXT_MAX_CHARS, 10_000);
    assert_eq!(DISPLAY_TEXT_BYTES_THRESHOLD, 40_000);
}

#[test]
fn truncate_passes_through_text_one_byte_under_the_threshold() {
    let s = "a".repeat(DISPLAY_TEXT_BYTES_THRESHOLD - 1);
    assert_eq!(truncate_for_display(&s), s);
}

#[test]
fn truncate_cuts_text_exactly_at_the_byte_threshold() {
    let s = "a".repeat(DISPLAY_TEXT_BYTES_THRESHOLD);
    let out = truncate_for_display(&s);
    let expected_remaining = DISPLAY_TEXT_BYTES_THRESHOLD - DISPLAY_TEXT_MAX_CHARS;
    assert_eq!(
        out,
        format!(
            "{}… (+{expected_remaining} more bytes)",
            "a".repeat(DISPLAY_TEXT_MAX_CHARS)
        )
    );
}

#[test]
fn truncate_handles_multibyte_boundary() {
    let s = "🦀".repeat(30_000);
    let out = truncate_for_display(&s);
    assert!(std::str::from_utf8(out.as_bytes()).is_ok());
    assert!(out.ends_with("(+80000 more bytes)"));
}

#[test]
fn a_large_binary_value_shows_its_exact_byte_count() {
    let mut blob = vec![0xFFu8; 100_001];
    blob[0] = 0x00;
    assert_eq!(value_to_display_text(&Value::Bytes(blob)), "<100001 bytes>");
    assert_eq!(value_to_display_text(&Value::Bytes(vec![0x00])), "<1 bytes>");
}

#[test]
fn text_preview_shows_nul_without_cutting_the_sample_or_changing_its_value() {
    let preview = crate::ui::row_object::CellPreview {
        value: Value::Text("before\0after".into()),
        byte_count: 9000,
    };
    let view = CellView::from_preview(&preview);
    assert_eq!(view.text_for_bind(false), "before␀after… (9000 bytes total)");
    assert!(!view.inline_editable);
    assert_eq!(preview.value, Value::Text("before\0after".into()));
}

#[test]
fn preview_matrix_bounds_layout_at_unicode_and_length_boundaries() {
    for unit in ["x", "é", "😀", "\0"] {
        for count in [511, 512, 513, 8_192] {
            let text = unit.repeat(count);
            for value in [Value::Text(text.clone()), Value::JsonText(text.clone())] {
                let preview = crate::ui::row_object::CellPreview {
                    value: value.clone(),
                    byte_count: 1_000_000,
                };
                let view = CellView::from_preview(&preview);
                let sample: String = text.chars().take(512).collect();
                assert_eq!(
                    view.text_for_bind(false),
                    format!("{}… (1000000 bytes total)", sample.replace('\0', "␀"))
                );
                assert_eq!(preview.value, value);
                assert!(!view.inline_editable);
            }
        }
    }
}

#[test]
fn oversized_text_previews_bound_layout_without_shortening_the_stored_sample() {
    for value in [Value::Text("x".repeat(8192)), Value::JsonText("x".repeat(8192))] {
        let preview = crate::ui::row_object::CellPreview {
            value: value.clone(),
            byte_count: 1_000_000,
        };
        let view = CellView::from_preview(&preview);
        assert_eq!(
            view.text_for_bind(false),
            format!("{}… (1000000 bytes total)", "x".repeat(512))
        );
        assert!(!view.inline_editable);
        assert_eq!(preview.value, value);
    }
}

#[test]
fn the_visible_preview_limit_counts_unicode_characters_and_preserves_nul_markers() {
    let text = format!("{}\0😀tail", "é".repeat(511));
    let preview = crate::ui::row_object::CellPreview {
        value: Value::Text(text.clone()),
        byte_count: 9000,
    };
    assert_eq!(
        CellView::from_preview(&preview).display_text,
        format!("{}␀… (9000 bytes total)", "é".repeat(511))
    );
    assert_eq!(preview.value, Value::Text(text));
}

#[test]
fn value_preview_shows_sample_and_original_byte_count() {
    let preview = crate::ui::row_object::CellPreview {
        value: Value::Text("sample".into()),
        byte_count: 9000,
    };
    assert_eq!(preview_to_display_text(&preview), "sample… (9000 bytes total)");
}

#[test]
fn multiline_preview_collapses_line_breaks_without_changing_the_value() {
    let text = "first\nsecond\r\nthird\u{2028}fourth\tlast";
    let preview = crate::ui::row_object::CellPreview {
        value: Value::Text(text.into()),
        byte_count: text.len(),
    };

    let view = CellView::from_preview(&preview);

    assert_eq!(
        view.display_text,
        format!("first↵second␍↵third↵fourth⇥last… ({} bytes total)", text.len())
    );
    assert_eq!(preview.value, Value::Text(text.into()));
}

#[test]
fn utf8_bytes_past_the_display_limit_mark_the_truncation_with_the_exact_remainder() {
    let text = "b".repeat(DISPLAY_TEXT_BYTES_THRESHOLD + 7);
    let display = value_to_display_text(&Value::Bytes(text.into_bytes()));
    let remaining = DISPLAY_TEXT_BYTES_THRESHOLD + 7 - DISPLAY_TEXT_MAX_CHARS;
    assert_eq!(
        display,
        format!("{}… (+{remaining} more bytes)", "b".repeat(DISPLAY_TEXT_MAX_CHARS))
    );
}

#[test]
fn display_text_truncates_huge_text_value() {
    let huge = "x".repeat(1_000_000);
    let display = value_to_display_text(&Value::Text(huge));
    assert!(display.len() < 100_000);
    assert!(display.ends_with("(+990000 more bytes)"));
}

#[test]
fn large_json_display_stops_at_the_display_budget() {
    let value = Value::Json(Box::new(serde_json::json!({"payload": "x".repeat(1_000_000)})));
    let original = value.clone();
    let display = value_to_display_text(&value);

    assert!(display.starts_with("{\"payload\":\""));
    assert!(display.len() <= DISPLAY_TEXT_BYTES_THRESHOLD + "…".len());
    assert!(display.chars().count() <= DISPLAY_TEXT_MAX_CHARS + 1);
    assert!(display.ends_with('…'));
    assert_eq!(value, original);
}

#[test]
fn json_below_the_byte_threshold_keeps_its_complete_display() {
    let payload = "x".repeat(DISPLAY_TEXT_MAX_CHARS * 2);
    let value = Value::Json(Box::new(serde_json::json!({"payload": payload})));
    let display = value_to_display_text(&value);

    assert_eq!(
        display,
        format!("{{\"payload\":\"{}\"}}", "x".repeat(DISPLAY_TEXT_MAX_CHARS * 2))
    );
}

#[test]
fn large_json_display_truncates_only_at_a_utf8_boundary() {
    let value = Value::Json(Box::new(serde_json::json!({
        "payload": "💾".repeat(DISPLAY_TEXT_BYTES_THRESHOLD)
    })));
    let display = value_to_display_text(&value);

    assert!(display.ends_with('…'));
    assert!(display.len() <= DISPLAY_TEXT_BYTES_THRESHOLD + "…".len());
    assert!(!display.contains('\u{fffd}'));
}

#[test]
fn large_text_sanitizes_visible_nul_without_changing_omitted_data() {
    let mut text = "x".repeat(100_000);
    text.replace_range(1..2, "\0");
    let original = Value::Text(text);
    let display = value_to_display_text(&original);

    assert!(display.starts_with("x␀"));
    assert!(display.ends_with("(+90000 more bytes)"));
    assert_eq!(original, Value::Text(format!("x\0{}", "x".repeat(99_998))));
}

#[test]
fn grid_widget_data_keys_use_distinct_quarks() {
    let quarks = [
        PositionKey::quark(),
        SnapshotKey::quark(),
        RowKeyKey::quark(),
        ColumnKey::quark(),
        SuppressKey::quark(),
        PopoverKey::quark(),
        PreeditKey::quark(),
        FullEditTextKey::quark(),
        IsNullKey::quark(),
    ];
    for (index, quark) in quarks.iter().enumerate() {
        for (other_index, other) in quarks.iter().enumerate() {
            if index != other_index {
                assert_ne!(quark, other);
            }
        }
    }
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn typed_widget_slots_round_trip_and_keep_same_value_types_apart() {
    gtk4::init().unwrap();
    let label = gtk4::Label::new(None);
    POSITION_SLOT.set(&label, 7);
    COLUMN_SLOT.set(&label, 3);
    assert_eq!(POSITION_SLOT.get(&label), Some(7));
    assert_eq!(COLUMN_SLOT.get(&label), Some(3));

    SUPPRESS_SLOT.set(&label, true);
    PREEDIT_SLOT.set(&label, false);
    IS_NULL_SLOT.set(&label, true);
    assert_eq!(SUPPRESS_SLOT.get(&label), Some(true));
    assert_eq!(PREEDIT_SLOT.get(&label), Some(false));
    assert_eq!(IS_NULL_SLOT.get(&label), Some(true));

    let row_key = vec![Value::Int(11), Value::Text("pk".into())];
    ROW_KEY_SLOT.set(&label, row_key.clone());
    assert_eq!(ROW_KEY_SLOT.cloned(&label), Some(row_key));

    FULL_EDIT_TEXT_SLOT.set(&label, "pending".into());
    assert_eq!(FULL_EDIT_TEXT_SLOT.cloned(&label), Some("pending".into()));
    assert_eq!(FULL_EDIT_TEXT_SLOT.take(&label), Some("pending".into()));
    assert_eq!(FULL_EDIT_TEXT_SLOT.cloned(&label), None);

    assert_eq!(POSITION_SLOT.take(&label), Some(7));
    assert_eq!(POSITION_SLOT.get(&label), None);
    assert_eq!(COLUMN_SLOT.get(&label), Some(3));
    assert_eq!(SUPPRESS_SLOT.get(&label), Some(true));
    assert_eq!(PREEDIT_SLOT.get(&label), Some(false));
    assert_eq!(IS_NULL_SLOT.get(&label), Some(true));
}
