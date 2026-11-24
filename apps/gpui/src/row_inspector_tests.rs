use gpui::TestAppContext;

use crate::GridView;
use crate::row_inspector_ui::{TableKeys, count_label, foreign_key_columns, pretty_single_row};
use crate::save_ui_tests::{fixture_copy, saving_view, write_session};

fn wait_for(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, ready: impl Fn(&GridView) -> bool) {
    for _ in 0..200 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        if view.read_with(cx, |view, _| ready(view)) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn add_notes(path: &std::path::Path) {
    let write = write_session(path, false);
    let cancel = tokio_util::sync::CancellationToken::new();
    write
        .apply(
            &[(
                "CREATE TABLE notes(id INTEGER PRIMARY KEY, order_id INTEGER REFERENCES orders(id), body TEXT)".into(),
                Vec::new(),
            )],
            &[],
            &cancel,
        )
        .ok()
        .expect("create the child table");
    write
        .apply(
            &[(
                "INSERT INTO notes(order_id, body) VALUES (1, 'first'), (1, 'second'), (2, 'other')".into(),
                Vec::new(),
            )],
            &[],
            &cancel,
        )
        .ok()
        .expect("insert child rows");
}

#[gpui::test]
fn row_details_count_the_rows_that_reference_the_active_row(cx: &mut TestAppContext) {
    let path = fixture_copy("row-references");
    add_notes(&path);
    let (view, cx) = saving_view(cx, &path);
    view.update(cx, |view, cx| {
        view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 0 });
        view.row_detail_open = true;
        cx.notify();
    });
    wait_for(&view, cx, |view| !view.row_inspector.references.is_empty());
    let references = view.read_with(cx, |view, _| view.row_inspector.references.clone());
    assert_eq!(references.len(), 1, "{references:?}");
    assert!(references[0].label.starts_with("notes"), "{references:?}");
    assert_eq!(references[0].count, Ok((2, false)), "{references:?}");
    assert!(cx.debug_bounds("row-detail-referenced-by").is_some());

    view.update(cx, |view, cx| view.open_reference(0, cx));
    wait_for(&view, cx, |view| view.grid_is_query && view.dataset_count() == 2);
    view.read_with(cx, |view, _| assert_eq!(view.dataset_count(), 2));
    std::fs::remove_file(path).ok();
}

#[gpui::test]
fn row_details_switch_to_a_json_view_of_the_row(cx: &mut TestAppContext) {
    let path = fixture_copy("row-json");
    let (view, cx) = saving_view(cx, &path);
    view.update(cx, |view, cx| {
        view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 0 });
        view.row_detail_open = true;
        view.row_inspector.json = true;
        cx.notify();
    });
    cx.run_until_parked();
    let json = view
        .read_with(cx, |view, _| view.row_detail_json_text())
        .expect("a row is active");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(parsed["id"], serde_json::json!(1), "{json}");
    assert!(json.contains('\n'), "pretty printed: {json}");
    assert!(cx.debug_bounds("row-detail-json").is_some());
    std::fs::remove_file(path).ok();
}

#[gpui::test]
fn alt_enter_opens_the_full_value_editor_for_a_short_value(cx: &mut TestAppContext) {
    let path = fixture_copy("alt-enter");
    let (view, cx) = saving_view(cx, &path);
    view.update(cx, |view, cx| {
        view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 1 });
        cx.notify();
    });
    cx.simulate_keystrokes("alt-enter");
    view.read_with(cx, |view, _| {
        let editing = view.cell_edit.active.as_ref().expect("an editor is open");
        assert!(editing.full_value, "Alt+Enter opens the full value editor");
        assert_eq!(editing.column, 1);
    });
    std::fs::remove_file(path).ok();
}

#[gpui::test]
fn wrap_text_toggles_from_the_palette_and_foreign_key_columns_are_links(cx: &mut TestAppContext) {
    let path = fixture_copy("wrap-links");
    let (view, cx) = saving_view(cx, &path);
    cx.update(|window, cx| view.update(cx, |view, cx| view.run_palette_id("wrap-text", window, cx)));
    view.read_with(cx, |view, _| assert!(view.wrap_text));
    cx.update(|window, cx| view.update(cx, |view, cx| view.run_palette_id("wrap-text", window, cx)));
    view.read_with(cx, |view, _| assert!(!view.wrap_text));

    view.update(cx, |view, _| {
        let table = view.catalog.grid_table.clone();
        let names: Vec<String> = view.source.columns().iter().map(|column| column.name.clone()).collect();
        view.row_inspector.keys = Some(TableKeys {
            table,
            foreign_key_columns: vec![names[1].clone()],
            referenced_by: Vec::new(),
        });
        let links = view.link_columns();
        assert_eq!(links.len(), names.len());
        assert!(links[1] && !links[0]);
        let control = gpui::Modifiers {
            control: true,
            ..Default::default()
        };
        assert!(view.follows_link(1, control));
        assert!(!view.follows_link(0, control));
        assert!(!view.follows_link(1, gpui::Modifiers::default()));
    });
    std::fs::remove_file(path).ok();
}

#[test]
fn helpers_dedupe_key_columns_pretty_print_one_row_and_cap_counts() {
    let key = |columns: &[&str]| bookie_core::ForeignKeyInfo {
        name: "fk".into(),
        columns: columns.iter().map(|column| column.to_string()).collect(),
        ref_schema: None,
        ref_table: "t".into(),
        ref_columns: Vec::new(),
        on_delete: None,
        on_update: None,
    };
    assert_eq!(foreign_key_columns(&[key(&["b", "a"]), key(&["a"])]), ["a", "b"]);
    assert_eq!(pretty_single_row(r#"[{"id":1}]"#), "{\n  \"id\": 1\n}");
    assert_eq!(count_label(&Ok((1000, true))), "1000+ rows");
    assert_eq!(count_label(&Ok((1, false))), "1 row");
    assert_eq!(count_label(&Err("denied".into())), "denied");
}
