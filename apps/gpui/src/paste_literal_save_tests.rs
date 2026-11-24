use super::*;

#[gpui::test]
fn literal_cell_paste_preserves_clipboard_text_through_guarded_save(cx: &mut TestAppContext) {
    let path = fixture_copy("paste-literal-cell");
    let (view, cx) = saving_view(cx, &path);
    let value = "first\tsecond\r\nthird 💾\n";
    view.read_with(cx, |view, _| {
        assert!(
            view.paste_actions()
                .iter()
                .any(|action| action.id == "paste-literal-cell" && action.enabled)
        );
    });
    cx.update(|window, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("x".repeat(1024 * 1024 + 1)));
        view.update(cx, |view, cx| {
            view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 1 });
            assert!(view.execute_tool_palette_action("paste-literal-cell", window, cx));
        });
    });
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.edits.is_empty(), "{}", view.status);
        assert!(view.status.contains("larger than 1 MiB"), "{}", view.status);
    });
    cx.update(|window, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(value.into()));
        view.update(cx, |view, cx| {
            view.cell_cursor = Some(bookie_results::grid_cursor::Cursor {
                row: usize::MAX,
                column: 1,
            });
            assert!(view.execute_tool_palette_action("paste-literal-cell", window, cx));
        });
    });
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.edits.is_empty(), "{}", view.status);
        assert!(view.status.contains("row is no longer available"), "{}", view.status);
    });
    cx.update(|window, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(value.into()));
        view.update(cx, |view, cx| {
            view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 0 });
            assert!(view.execute_tool_palette_action("paste-literal-cell", window, cx));
        });
    });
    view.read_with(cx, |view, _| {
        assert!(view.cell_edit.edits.is_empty(), "{}", view.status);
        assert!(view.status.contains("only for text columns"), "{}", view.status);
    });
    cx.update(|window, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(value.into()));
        view.update(cx, |view, cx| {
            view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 1 });
            assert!(view.execute_tool_palette_action("paste-literal-cell", window, cx));
        });
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.cell_edit.edits.len(), 1, "{}", view.status);
        assert_eq!(
            view.cell_edit.edits.get(&(0, 1)),
            Some(&bookie_gpui::model::Cell::Text(value.into())),
            "{}",
            view.status
        );
    });
    cx.simulate_keystrokes("ctrl-s");
    settle_saving(cx);
    cx.update(|window, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("blocked".into()));
        view.update(cx, |view, cx| {
            assert!(view.execute_tool_palette_action("paste-literal-cell", window, cx));
        });
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cell_edit.edits.get(&(0, 1)),
            Some(&bookie_gpui::model::Cell::Text(value.into()))
        );
        assert!(view.status.contains("review before pasting"), "{}", view.status);
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    settle_saving(cx);
    assert_eq!(saved_customer(&path, 1), value);
    cx.update(|window, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new()));
        view.update(cx, |view, cx| {
            view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 1 });
            assert!(view.execute_tool_palette_action("paste-literal-cell", window, cx));
        });
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cell_edit.edits.get(&(0, 1)),
            Some(&bookie_gpui::model::Cell::Text(String::new()))
        );
    });
    cx.simulate_keystrokes("ctrl-s");
    settle_saving(cx);
    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    settle_saving(cx);
    assert_eq!(saved_customer(&path, 1), "");
    std::fs::remove_file(&path).ok();
}
