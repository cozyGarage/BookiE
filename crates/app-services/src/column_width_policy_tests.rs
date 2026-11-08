use super::*;

#[test]
fn text_widths_take_the_widest_value_within_bounds() {
    assert_eq!(default_width_cells("text", ["a", "bb"]), 4);
    assert_eq!(default_width_cells("text", ["a", "twelve chars"]), 12);
    let long = "x".repeat(500);
    assert_eq!(default_width_cells("text", [long.as_str()]), 40);
    assert_eq!(default_width_cells("text", ["a"]), 4);
    assert_eq!(default_width_cells("text", ["", ""]), 4);
}

#[test]
fn a_wide_character_takes_two_cells() {
    assert_eq!(default_width_cells("text", ["東京都千代田区"]), 14);
    let wide = "東".repeat(500);
    assert_eq!(
        sampled_measure_texts(WidthKind::Text, [wide.as_str()])
            .iter()
            .map(|text| text.chars().count())
            .collect::<Vec<_>>(),
        [20]
    );
    assert_eq!(
        typical_measure_texts("enum('東東東東東東東東東東東東東東東東東東東東東東東東東東東東東東')")
            .iter()
            .map(|text| text.chars().count())
            .collect::<Vec<_>>(),
        [20]
    );
}

#[test]
fn every_value_beyond_plain_ascii_is_measured() {
    assert_eq!(
        sampled_measure_texts(
            WidthKind::Text,
            ["Marunouchi 12", "कखगघ", "東京", "a", "कखगघ", "tab\tbed"]
        ),
        ["tab\tbed", "कखगघ", "東京", "Marunouchi 12"]
    );
}

#[test]
fn the_longest_plain_value_is_measured_within_the_cap() {
    assert_eq!(
        sampled_measure_texts(WidthKind::Text, ["CF-1001", "", "a"]),
        ["CF-1001"]
    );
    let long = "x".repeat(500);
    assert_eq!(
        sampled_measure_texts(WidthKind::Text, [long.as_str()])
            .iter()
            .map(|text| text.chars().count())
            .collect::<Vec<_>>(),
        [40]
    );
    assert!(sampled_measure_texts(WidthKind::Text, [""]).is_empty());
    assert!(sampled_measure_texts(WidthKind::Number, ["7"]).is_empty());
}

#[test]
fn only_the_first_200_rows_are_sampled() {
    let mut values: Vec<String> = (0..200).map(|_| "short".to_owned()).collect();
    values.push("x".repeat(30));
    let refs: Vec<&str> = values.iter().map(String::as_str).collect();
    assert_eq!(default_width_cells("text", refs), 5);
}

#[test]
fn a_number_is_never_cut() {
    assert_eq!(default_width_cells("bigint", ["7"]), 4);
    assert_eq!(default_width_cells("bigint", ["-1234567890"]), 11);
    let wide = "1234567890123456789012345678901234567890.5";
    assert_eq!(default_width_cells("numeric", [wide]), 42);
}

#[test]
fn an_untyped_column_of_numbers_is_never_cut() {
    let wide = "9".repeat(50);
    assert_eq!(default_width_cells("text", [wide.as_str()]), 40);
    assert_eq!(number_width_cells([wide.as_str()]), 50);
}

#[test]
fn a_bool_is_as_wide_as_false() {
    assert_eq!(default_width_cells("boolean", ["true"]), 5);
    assert_eq!(default_width_cells("boolean", [] as [&str; 0]), 5);
}

#[test]
fn a_temporal_column_is_as_wide_as_its_format() {
    assert_eq!(default_width_cells("date", [] as [&str; 0]), 10);
    assert_eq!(default_width_cells("date", ["2026-04-26"]), 10);
    assert_eq!(default_width_cells("timestamp with time zone", [] as [&str; 0]), 25);
}

#[test]
fn with_no_rows_a_column_opens_at_what_its_type_leads_one_to_expect() {
    assert_eq!(typical_width_cells("text"), 20);
    assert_eq!(typical_width_cells("varchar(8)"), 8);
    assert_eq!(typical_width_cells("varchar(255)"), 20);
    assert_eq!(typical_width_cells("uuid"), 36);
    assert_eq!(typical_width_cells("bigint"), 10);
    assert_eq!(typical_width_cells("numeric(10,2)"), 12);
    assert_eq!(typical_width_cells("enum('active','suspended')"), 9);
    assert_eq!(typical_width_cells("set('a','b')"), 20);
    assert_eq!(typical_width_cells("timestamp with time zone"), 25);
    assert_eq!(typical_width_cells("boolean"), 5);
    assert_eq!(typical_width_cells("text"), 20);
}

#[test]
fn the_typical_texts_are_the_values_a_type_spells_out() {
    assert_eq!(typical_measure_texts("date"), ["0000-00-00"]);
    assert_eq!(
        typical_measure_texts("uuid")
            .iter()
            .map(|text| text.chars().count())
            .collect::<Vec<_>>(),
        [36]
    );
    assert_eq!(
        typical_measure_texts("enum('active','suspended')"),
        ["active", "suspended"]
    );
    assert!(typical_measure_texts("text").is_empty());
}

#[test]
fn header_text_is_capped_for_the_shell_to_measure() {
    assert_eq!(header_measure_text("note"), "note");
    assert_eq!(header_measure_text(&"n".repeat(50)).chars().count(), 40);
}

#[test]
fn width_kind_follows_catalog_type_and_value() {
    assert_eq!(width_kind_for_data_type("boolean"), WidthKind::Bool);
    assert_eq!(width_kind_for_data_type("numeric(10,2)"), WidthKind::Number);
    assert_eq!(width_kind_for_data_type("date"), WidthKind::Temporal);
    assert_eq!(width_kind_for_data_type("varchar(32)"), WidthKind::Text);
    assert_eq!(width_kind_for_value(&Value::Int(1)), WidthKind::Number);
    assert_eq!(width_kind_for_value(&Value::Text("a".into())), WidthKind::Text);
}
