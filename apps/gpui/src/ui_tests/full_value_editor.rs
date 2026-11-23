use bookie_gpui::model::Cell;
use gpui::TestAppContext;

use super::open;

#[gpui::test]
fn opening_a_multiline_staged_text_edit_uses_the_complete_effective_value(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let full_value = "first line\r\nsecond line 💾";
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit.edits.insert((2, 1), Cell::Text(full_value.into()));
            view.start_edit(2, 1, window, cx);
        });
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("full-value-editor-dialog").is_some(),
        "the multiline editor should be visible"
    );
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.active.is_some(), "multiline text should open an editor");
        assert_ne!(
            view.status,
            "Multiline values cannot be edited in this single-line prototype"
        );
    });
    view.read_with(cx, |view, cx| {
        assert_eq!(view.cell_edit.value_editor.read(cx).value().as_ref(), full_value);
    });
}

#[gpui::test]
fn applying_a_full_value_preserves_line_endings_and_unicode(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let replacement = "first\r\nsecond\nthird 💾";
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit.edits.insert((2, 1), Cell::Text("old\nvalue".into()));
            view.start_edit(2, 1, window, cx);
            view.cell_edit
                .value_editor
                .update(cx, |editor, cx| editor.set_value(replacement, window, cx));
            view.apply_active_cell_edit(window, cx);
        });
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_edit.edits.get(&(2, 1)), Some(&Cell::Text(replacement.into())));
        assert!(view.cell_edit.active.is_none());
    });
}

#[gpui::test]
fn cancelling_a_full_value_edit_leaves_the_previous_staged_value(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let previous = Cell::Text("staged\nvalue".into());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit.edits.insert((2, 1), previous.clone());
            view.start_edit(2, 1, window, cx);
            view.cancel_active_cell_edit(window, cx);
        });
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_edit.edits.get(&(2, 1)), Some(&previous));
        assert!(view.cell_edit.active.is_none());
    });
}

#[gpui::test]
fn invalid_json_full_value_does_not_replace_the_staged_value(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let previous = Cell::Json(Box::new(serde_json::json!({"ok": true})));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit.edits.insert((2, 3), previous.clone());
            view.start_edit(2, 3, window, cx);
            view.cell_edit
                .value_editor
                .update(cx, |editor, cx| editor.set_value("{\"ok\":}", window, cx));
            view.apply_active_cell_edit(window, cx);
        });
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_edit.edits.get(&(2, 3)), Some(&previous));
        assert!(view.cell_edit.active.as_ref().is_some_and(|editing| editing.full_value));
        assert!(view.status.starts_with("Not applied:"));
    });
}

#[gpui::test]
fn empty_text_remains_distinct_from_json_null(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit.edits.insert((2, 1), Cell::Text(String::new()));
            view.start_edit(2, 1, window, cx);
            view.apply_active_cell_edit(window, cx);
        });
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_edit.edits.get(&(2, 1)), Some(&Cell::Text(String::new())));
    });
}

#[gpui::test]
fn applying_json_null_keeps_the_json_value_kind(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let json_null = Cell::Json(Box::new(serde_json::Value::Null));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit.edits.insert((2, 3), json_null.clone());
            view.start_edit(2, 3, window, cx);
            assert_eq!(view.cell_edit.value_editor.read(cx).value().as_ref(), "null");
            view.apply_active_cell_edit(window, cx);
        });
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_edit.edits.get(&(2, 3)), Some(&json_null));
    });
}

#[gpui::test]
fn oversized_full_value_refuses_edit_without_replacing_the_staged_value(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let previous = Cell::Text(format!("{}\n", "x".repeat(1024 * 1024)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit.edits.insert((2, 1), previous.clone());
            view.start_edit(2, 1, window, cx);
        });
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_edit.edits.get(&(2, 1)), Some(&previous));
        assert!(view.cell_edit.active.is_none());
        assert!(view.status.contains("1 MiB"));
    });
}

#[gpui::test]
fn oversized_full_value_replacement_is_rejected_without_closing_editor(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let previous = Cell::Text("old\nvalue".into());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.cell_edit.edits.insert((2, 1), previous.clone());
            view.start_edit(2, 1, window, cx);
            view.cell_edit.value_editor.update(cx, |editor, cx| {
                editor.set_value(&"x".repeat(1024 * 1024 + 1), window, cx);
            });
            view.apply_active_cell_edit(window, cx);
        });
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_edit.edits.get(&(2, 1)), Some(&previous));
        assert!(view.cell_edit.active.as_ref().is_some_and(|editing| editing.full_value));
        assert!(view.status.contains("1 MiB"));
    });
}

#[gpui::test]
fn opening_a_multiline_value_preserves_it_in_the_full_value_editor(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let value = Cell::Text("first\nsecond".into());
    view.update(cx, |view, _| {
        view.cell_edit.edits.insert((0, 1), value.clone());
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.start_edit(0, 1, window, cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("full-value-editor-dialog").is_some());
    view.read_with(cx, |view, cx| {
        assert!(view.cell_edit.active.as_ref().is_some_and(|editing| editing.full_value));
        assert_eq!(view.cell_edit.edits.get(&(0, 1)), Some(&value));
        assert_eq!(view.cell_edit.value_editor.read(cx).value().as_ref(), "first\nsecond");
    });
}
