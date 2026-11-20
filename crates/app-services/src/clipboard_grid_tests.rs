use super::*;

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
    use bookie_core::Value;

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

    let copied = bookie_core::export::render_tsv(&[], &rows, false);
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

mod roundtrip {
    use super::super::parse_clipboard_grid as parse_with_nulls;

    fn parse_clipboard_grid(text: &str) -> Vec<Vec<String>> {
        parse_with_nulls(text)
            .into_iter()
            .map(|row| row.into_iter().map(Option::unwrap).collect())
            .collect()
    }

    fn spreadsheet_encode(rows: &[Vec<String>]) -> String {
        rows.iter()
            .map(|row| {
                row.iter()
                    .map(|cell| {
                        if cell.contains(['\t', '\n', '\r', '"']) {
                            format!("\"{}\"", cell.replace('"', "\"\""))
                        } else {
                            cell.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn strings(alphabet: &[char], max_len: usize) -> Vec<String> {
        let mut all = vec![String::new()];
        let mut frontier = vec![String::new()];
        for _ in 0..max_len {
            frontier = frontier
                .iter()
                .flat_map(|prefix| alphabet.iter().map(move |c| format!("{prefix}{c}")))
                .collect();
            all.extend(frontier.iter().cloned());
        }
        all
    }

    #[test]
    fn whatever_a_spreadsheet_copies_is_pasted_back_unchanged() {
        let cells = strings(&['a', '"', '\t', '\n', ' '], 3);
        let mut failures = Vec::new();
        for first in &cells {
            for second in &cells {
                let grid = vec![vec![first.clone(), second.clone()]];
                let text = spreadsheet_encode(&grid);
                if text.is_empty() {
                    continue;
                }
                let parsed = parse_clipboard_grid(&text);
                if parsed != grid {
                    failures.push((grid, text, parsed));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} copies changed; first: {:?}",
            failures.len(),
            cells.len() * cells.len(),
            failures.first()
        );
    }
    #[test]
    fn carriage_returns_and_several_rows_survive_the_same_round_trip() {
        let with_returns = strings(&['a', '"', '\t', '\n', '\r', ' '], 2);
        for first in &with_returns {
            for second in &with_returns {
                let grid = vec![vec![first.clone(), second.clone()]];
                let text = spreadsheet_encode(&grid);
                if !text.is_empty() {
                    assert_eq!(parse_clipboard_grid(&text), grid, "{text:?}");
                }
            }
        }
        let small = strings(&['a', '"', '\t', '\n', '\r', ' '], 1);
        for a in &small {
            for b in &small {
                for c in &small {
                    for d in &small {
                        let grid = vec![vec![a.clone(), b.clone()], vec![c.clone(), d.clone()]];
                        let text = spreadsheet_encode(&grid);
                        assert_eq!(parse_clipboard_grid(&text), grid, "{text:?}");
                    }
                }
            }
        }
    }
}
