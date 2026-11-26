use gpui::{Focusable, TestAppContext};
use gpui_kit::test::TestWindowExt;

use crate::GridView;
use crate::save_ui_tests::{saving_view, settle_saving};
use crate::ui::keys::Surface;
use crate::ui_tests::test_kit::orders;
use bookie_app_services::structure_change_plan::StructureChangeKind;

type Opener = fn(&mut GridView, &mut gpui::Context<GridView>);

fn openers() -> Vec<(Surface, Opener)> {
    vec![
        (Surface::Import, |view, cx| view.open_import_bar(cx)),
        (Surface::ExportFile, |view, cx| view.open_export_file_bar(cx)),
        (Surface::SchemaDiagram, |view, cx| view.open_schema_diagram_bar(cx)),
        (Surface::Insert, |view, cx| view.open_insert_form(cx)),
        (Surface::Structure, |view, cx| {
            view.open_structure_bar(StructureChangeKind::DropColumn, cx)
        }),
        (Surface::SetCells, |view, cx| {
            view.selection = Some(crate::Selection { anchor: 0, focus: 1 });
            view.cell_cursor = Some(bookie_results::grid_cursor::Cursor { row: 0, column: 1 });
            view.open_set_cells_bar(cx);
        }),
        (Surface::SqlFile, |view, cx| view.open_sql_file_bar(false, cx)),
        (Surface::Backup, |view, _| view.search.backup.open = true),
        (Surface::TablesExport, |view, _| view.search.tables_export.open = true),
        (Surface::Resolve, |view, _| view.resolve.open = true),
    ]
}

fn open_surface(view: &gpui::Entity<GridView>, cx: &mut gpui::VisualTestContext, opener: Opener) {
    cx.update(|_, cx| view.update(cx, opener));
    settle_saving(cx);
    cx.update(|window, cx| window.render_frame(cx));
}

#[gpui::test]
fn escape_closes_every_one_shot_dialog_and_changes_nothing(cx: &mut TestAppContext) {
    let fixture = orders();
    let path = fixture.to_path_buf();
    let (view, cx) = saving_view(cx, &path);
    for (surface, opener) in openers() {
        open_surface(&view, cx, opener);
        view.read_with(cx, |view, _| {
            assert_eq!(view.top_surface(), Some(surface), "{}", view.status)
        });
        cx.simulate_keystrokes("escape");
        settle_saving(cx);
        cx.update(|window, cx| window.render_frame(cx));
        view.read_with(cx, |view, _| {
            assert_eq!(view.top_surface(), None, "{surface:?} stays open");
            assert!(view.cell_edit.edits.is_empty(), "{surface:?} changed a cell");
            assert!(view.save.pending.is_none(), "{surface:?} started a write");
        });
        assert!(cx.debug_bounds("bar-dialog").is_none(), "{surface:?} is still drawn");
    }
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn one_shot_steps_open_centred_and_leave_the_grid_where_it_was(cx: &mut TestAppContext) {
    let fixture = orders();
    let path = fixture.to_path_buf();
    let (view, cx) = saving_view(cx, &path);
    cx.update(|window, cx| window.render_frame(cx));
    let grid = cx.debug_bounds("grid-pager").expect("the grid is drawn");
    let rendered = [Surface::Structure, Surface::SetCells, Surface::SqlFile];
    for (surface, opener) in openers().into_iter().filter(|(surface, _)| rendered.contains(surface)) {
        open_surface(&view, cx, opener);
        assert!(cx.debug_bounds("bar-dialog").is_some(), "{surface:?} opens as a dialog");
        let moved = cx.debug_bounds("grid-pager").expect("the grid stays drawn");
        assert_eq!(moved.origin, grid.origin, "{surface:?} pushed the grid");
        cx.simulate_keystrokes("escape");
        settle_saving(cx);
    }
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn enter_submits_a_safe_step(cx: &mut TestAppContext) {
    let fixture = orders();
    let path = fixture.to_path_buf();
    let (view, cx) = saving_view(cx, &path);
    let (_, opener) = openers()
        .into_iter()
        .find(|(surface, _)| *surface == Surface::SetCells)
        .expect("set cells is listed");
    open_surface(&view, cx, opener);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let input = view.cell_edit.set.input.clone().expect("the value field");
            input.update(cx, |input, cx| input.set_content("bulk", cx));
        })
    });
    cx.simulate_keystrokes("enter");
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.top_surface(), None, "{}", view.status);
        assert_eq!(view.cell_edit.edits.len(), 2, "{}", view.status);
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn enter_does_not_confirm_a_destructive_review(cx: &mut TestAppContext) {
    let fixture = orders();
    let path = fixture.to_path_buf();
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.open_structure_bar(StructureChangeKind::DropTable, cx)
        })
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let input = view.structure.input.clone().expect("the structure field");
            input.update(cx, |input, cx| input.set_content("orders", cx));
        })
    });
    cx.simulate_keystrokes("enter");
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        let pending = view.save.pending.as_ref().expect("enter opens the review");
        assert!(pending.destructive);
    });
    cx.simulate_keystrokes("enter");
    settle_saving(cx);
    view.read_with(cx, |view, _| {
        assert!(view.save.pending.is_some(), "enter does not drop the table");
    });
    cx.simulate_keystrokes("escape");
    settle_saving(cx);
    view.read_with(cx, |view, _| assert_eq!(view.top_surface(), Some(Surface::Structure)));
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn the_host_key_prompt_cancels_on_escape_and_has_no_enter_default(cx: &mut TestAppContext) {
    let fixture = orders();
    let path = fixture.to_path_buf();
    let (view, cx) = saving_view(cx, &path);
    let (respond, mut answer) = tokio::sync::oneshot::channel();
    let pending = bookie_app_services::ssh_prompt::PendingHostKey {
        host: "db.example".into(),
        algorithm: "ssh-ed25519".into(),
        fingerprint: "SHA256:abc".into(),
        respond,
    };
    cx.update(|_, cx| view.update(cx, |view, _| view.host_key.current = Some(pending)));
    cx.update(|window, cx| window.render_frame(cx));
    assert!(cx.debug_bounds("host-key-dialog").is_some());
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| assert_eq!(view.top_surface(), Some(Surface::HostKey)));
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert_eq!(view.top_surface(), None));
    assert_eq!(answer.try_recv().ok(), Some(false), "escape refuses the key");
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn escape_closes_the_connection_dialog_above_the_notification_drawer(cx: &mut TestAppContext) {
    let fixture = orders();
    let path = fixture.to_path_buf();
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.notices.open = true;
            view.open_connection_dialog(cx);
        })
    });
    cx.update(|window, cx| window.render_frame(cx));
    assert!(cx.debug_bounds("notifications-list").is_some());
    assert!(cx.debug_bounds("connection-dialog").is_some());

    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| {
        assert!(!view.connection_form.open, "Escape closes the topmost dialog");
        assert!(view.notices.open, "the covered notification drawer stays open");
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn escape_closes_notifications_without_stopping_a_focused_query(cx: &mut TestAppContext) {
    let fixture = orders();
    let path = fixture.to_path_buf();
    let (view, cx) = saving_view(cx, &path);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.notices.open = true;
            view.queries.cancel = tokio_util::sync::CancellationToken::new();
            view.panel.running = Some((view.queries.generation, std::time::Instant::now()));
            view.sql_input.update(cx, |editor, cx| editor.focus(window, cx));
        })
    });
    view.read_with(cx, |view, _| assert!(view.can_stop_query(), "the test query is active"));
    assert!(cx.update(|window, cx| view.read(cx).sql_input.read(cx).focus_handle(cx).is_focused(window)));
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| {
        assert!(!view.notices.open, "Escape closes the notification drawer");
        assert!(
            !view.queries.cancel.is_cancelled(),
            "closing the drawer does not stop the query"
        );
    });
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn escape_closes_notifications_above_the_palette_first(cx: &mut TestAppContext) {
    let fixture = orders();
    let path = fixture.to_path_buf();
    let (view, cx) = saving_view(cx, &path);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.notices.open = true;
            cx.notify();
        })
    });
    cx.simulate_keystrokes("ctrl-shift-p");
    view.read_with(cx, |view, _| assert!(view.palette.open));

    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| {
        assert!(
            !view.notices.open,
            "Escape closes the visually higher notification drawer"
        );
        assert!(view.palette.open, "the covered palette stays open");
    });
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(!view.palette.open));
    std::fs::remove_file(&path).ok();
}
