use super::model::*;
use super::selection::*;

#[test]
fn a_plain_click_selects_one_row_and_shift_extends_from_the_anchor() {
    let one = Selection::after_click(None, 10, false);
    assert_eq!((one.first(), one.last(), one.count()), (10, 10, 1));
    let extended = Selection::after_click(Some(one), 7, true);
    assert_eq!(
        (extended.anchor, extended.first(), extended.last(), extended.count()),
        (10, 7, 10, 4)
    );
    assert!(extended.contains(8) && !extended.contains(11));
    assert_eq!(Selection::after_click(Some(extended), 3, false).count(), 1);
    assert_eq!(Selection::after_click(None, 3, true).count(), 1);
}

#[test]
fn the_label_and_copy_limit_match_the_tauri_prototype() {
    assert_eq!(Selection::after_click(None, 0, false).label(), "1 row selected");
    let wide = Selection {
        anchor: 0,
        focus: 5_000,
    };
    assert!(!wide.copyable());
    assert_eq!(wide.label(), "5001 rows selected (copy is limited to 5000)");
    assert!(
        Selection {
            anchor: 0,
            focus: 4_999
        }
        .copyable()
    );
}

#[test]
fn tsv_fields_follow_the_core_export_quoting_rules() {
    assert_eq!(tsv_field(&Cell::Null), "\\N");
    assert_eq!(tsv_field(&Cell::Text(String::new())), "\"\"");
    assert_eq!(tsv_field(&Cell::Text("a\tb".into())), "\"a\tb\"");
    assert_eq!(tsv_field(&Cell::Text("say \"hi\"".into())), "\"say \"\"hi\"\"\"");
    assert_eq!(tsv_field(&Cell::Text("\\NN".into())), "\"\\NN\"");
    assert_eq!(tsv_field(&Cell::Text("plain".into())), "plain");
    assert_eq!(tsv_field(&Cell::Int(value_of(0))), "9007199254740993");
}

#[test]
fn copying_rows_joins_cells_with_tabs_and_rows_with_newlines() {
    let text = copy_tsv((0..2).map(row_at), true);
    let lines: Vec<&str> = text.split('\n').collect();
    assert_eq!(lines[0], "value\tlabel\tstate\ttyped");
    assert_eq!(lines.len(), 3);
    assert!(lines[1].starts_with("9007199254740993\tSample 0\t\\N\t"));
    assert_eq!(lines[1].split('\t').count(), 4);
    assert_eq!(copy_tsv((0..2).map(row_at), false).split('\n').count(), 2);
}
