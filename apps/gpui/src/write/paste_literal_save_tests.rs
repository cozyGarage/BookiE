use super::*;

#[gpui::test]
fn literal_cell_paste_preserves_clipboard_text_through_guarded_save(cx: &mut TestAppContext) {
    let path = fixture_copy("paste-literal-cell");
    let (view, cx) = saving_view(cx, &path);
    let write = write_session(&path, false);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let id = view.live.as_ref().and_then(|live| live.writable).expect("writable");
            view.save.write = Some((id, write));
        })
    });
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
    cx.simulate_keystrokes("secondary-s");
    wait_for_save_state(&view, cx, |view| view.save.pending.is_some());
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
    wait_for_save_state(
        &view,
        cx,
        |view| matches!(view.source.row(0).get(1), Some(bookie_gpui::model::Cell::Text(text)) if text == value),
    );
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
    cx.simulate_keystrokes("secondary-s");
    wait_for_save_state(&view, cx, |view| view.save.pending.is_some());
    cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
    wait_for_save_state(
        &view,
        cx,
        |view| matches!(view.source.row(0).get(1), Some(bookie_gpui::model::Cell::Text(text)) if text.is_empty()),
    );
    assert_eq!(saved_customer(&path, 1), "");
    std::fs::remove_file(&path).ok();
}

fn wait_for_save_state(
    view: &gpui::Entity<GridView>,
    cx: &mut gpui::VisualTestContext,
    ready: impl Fn(&GridView) -> bool,
) {
    for _ in 0..200 {
        settle_saving(cx);
        if view.read_with(cx, |view, _| ready(view)) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    view.read_with(cx, |view, _| {
        assert!(ready(view), "save state did not settle: {}", view.status)
    });
}
