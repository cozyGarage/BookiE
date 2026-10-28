use super::*;

#[test]
fn paste_text_size_is_bounded_before_parsing() {
    assert!(paste_size_allowed(MAX_PASTE_BYTES));
    assert!(!paste_size_allowed(MAX_PASTE_BYTES + 1));
}

fn grid(rows: &[&[&str]]) -> Vec<Vec<Option<String>>> {
    rows.iter()
        .map(|row| row.iter().map(|cell| Some(cell.to_string())).collect())
        .collect()
}

#[test]
fn pasting_copied_rows_undoes_the_formula_guard_and_null_marker() {
    let copied = "\"'-5\"\t\"'=1+1\"\t\\N\tplain\t'quoted\n";
    assert_eq!(
        parse_clipboard_grid(copied),
        vec![vec![
            Some("-5".into()),
            Some("=1+1".into()),
            None,
            Some("plain".into()),
            Some("'quoted".into())
        ]]
    );
}

#[test]
fn copied_rows_paste_back_with_null_empty_text_and_marker_text_unchanged() {
    use tablepro_core::Value;

    let texts = [
        "",
        "\\N",
        "\\NN",
        "\"\"",
        "\"\\N\"",
        "plain",
        "a\tb",
        "line1\nline2",
        "say \"hi\"",
        "'x",
        "NULL",
    ];
    let mut rows: Vec<Vec<Value>> = texts
        .iter()
        .map(|text| vec![Value::Text((*text).into()), Value::Null])
        .collect();
    rows.push(vec![Value::Null, Value::Text(String::new())]);
    let expected: Vec<Vec<Option<String>>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|value| match value {
                    Value::Text(text) => Some(text.clone()),
                    _ => None,
                })
                .collect()
        })
        .collect();

    let copied = tablepro_core::export::render_tsv(&[], &rows, false);
    assert_eq!(parse_clipboard_grid(&copied), expected, "{copied:?}");
}

#[test]
fn tab_separated_text_becomes_rows_and_cells() {
    assert_eq!(parse_clipboard_grid("a\tb\nc\td\n"), grid(&[&["a", "b"], &["c", "d"]]));
    assert_eq!(parse_clipboard_grid("a\tb\r\nc\td"), grid(&[&["a", "b"], &["c", "d"]]));
    assert_eq!(parse_clipboard_grid("only"), grid(&[&["only"]]));
    assert!(parse_clipboard_grid("").is_empty());
}

#[test]
fn spreadsheet_quoting_keeps_tabs_newlines_and_quotes_inside_one_cell() {
    assert_eq!(
        parse_clipboard_grid("\"a\tb\"\t\"line1\nline2\"\t\"say \"\"hi\"\"\"\n"),
        grid(&[&["a\tb", "line1\nline2", "say \"hi\""]])
    );
    assert_eq!(parse_clipboard_grid("x\t\ty\n"), grid(&[&["x", "", "y"]]));
    assert_eq!(parse_clipboard_grid("a\"b\tc"), grid(&[&["a\"b", "c"]]));
}

#[test]
fn a_leading_quote_on_ordinary_text_is_kept() {
    assert_eq!(parse_clipboard_grid("\"abc\""), grid(&[&["\"abc\""]]));
    assert_eq!(parse_clipboard_grid("\"a\", \"b\"\tx"), grid(&[&["\"a\", \"b\"", "x"]]));
    assert_eq!(parse_clipboard_grid("\"abc\tdef"), grid(&[&["\"abc", "def"]]));
    assert_eq!(parse_clipboard_grid("\"\"\n\"x\""), grid(&[&[""], &["\"x\""]]));
    assert_eq!(parse_clipboard_grid("\"a\tb\"x"), grid(&[&["\"a", "b\"x"]]));
}

#[test]
fn a_plan_places_every_cell_from_the_anchor_and_refuses_to_run_off_the_grid() {
    let plan = paste_plan(2, 1, &grid(&[&["a", "b"], &["c", "d"]]), 10, &[0, 1, 2, 3, 4]).unwrap();
    let places: Vec<(u32, usize, &str)> = plan
        .iter()
        .map(|c| (c.row, c.col, c.text.as_deref().unwrap()))
        .collect();
    assert_eq!(places, [(2, 1, "a"), (2, 2, "b"), (3, 1, "c"), (3, 2, "d")]);
    assert!(paste_plan(9, 0, &grid(&[&["a"], &["b"]]), 10, &[0, 1, 2, 3, 4]).is_err());
    assert!(paste_plan(0, 4, &grid(&[&["a", "b"]]), 10, &[0, 1, 2, 3, 4]).is_err());
    assert!(paste_plan(0, 0, &[], 10, &[0, 1, 2, 3, 4]).is_err());
}

#[test]
fn a_plan_follows_the_displayed_column_order() {
    let plan = paste_plan(0, 0, &grid(&[&["A", "B"]]), 1, &[0, 2, 1]).unwrap();
    let places: Vec<(usize, &str)> = plan.iter().map(|c| (c.col, c.text.as_deref().unwrap())).collect();
    assert_eq!(places, [(0, "A"), (2, "B")]);
}

#[test]
fn a_plan_skips_hidden_columns_and_refuses_a_hidden_anchor() {
    let plan = paste_plan(0, 0, &grid(&[&["A", "B"]]), 1, &[0, 2]).unwrap();
    let places: Vec<(usize, &str)> = plan.iter().map(|c| (c.col, c.text.as_deref().unwrap())).collect();
    assert_eq!(places, [(0, "A"), (2, "B")]);
    assert!(paste_plan(0, 1, &grid(&[&["A"]]), 1, &[0, 2]).is_err());
    assert!(paste_plan(0, 2, &grid(&[&["A", "B"]]), 1, &[0, 2]).is_err());
}

#[test]
fn a_huge_paste_is_refused_before_any_cell_is_touched() {
    let big = vec![vec![Some("x".to_string()); 100]; 100];
    assert!(paste_plan(0, 0, &big, 20_000, &(0..200).collect::<Vec<_>>()).is_err());
}

#[test]
fn a_pending_paste_requires_the_same_page_generation_and_target_row_identities() {
    use crate::services::change_tracker::RowKey;
    use tablepro_core::Value;

    let first = RowKey::from_pk_values(&[Value::Int(1)]).unwrap();
    let second = RowKey::from_pk_values(&[Value::Int(2)]).unwrap();
    let draft = RowKey::Draft(7);
    let generation = uuid::Uuid::new_v4();
    let snapshot = PasteSnapshot::new(
        Some(generation),
        vec![Some(first.clone()), Some(second.clone()), Some(draft.clone())],
    );
    let unchanged = [Some(first.clone()), Some(second.clone()), Some(draft.clone())];
    let reordered = [Some(second.clone()), Some(first.clone()), Some(draft.clone())];
    let replaced_draft = [Some(first.clone()), Some(second.clone()), Some(RowKey::Draft(8))];
    let missing = [Some(first.clone()), None, Some(draft)];

    assert!(snapshot.matches(Some(generation), &[0, 1, 2], |position| {
        unchanged[position as usize].clone()
    }));
    assert!(!snapshot.matches(Some(uuid::Uuid::new_v4()), &[0], |position| {
        unchanged[position as usize].clone()
    }));
    assert!(!snapshot.matches(Some(generation), &[0, 1], |position| {
        reordered[position as usize].clone()
    }));
    assert!(!snapshot.matches(Some(generation), &[2], |position| {
        replaced_draft[position as usize].clone()
    }));
    assert!(!snapshot.matches(Some(generation), &[0, 1], |position| missing[position as usize].clone()));
}
