use bookie_gpui::model::Cell;
use gpui::TestAppContext;

use super::open;

#[gpui::test]
fn failed_run_has_no_grid_data_and_preserves_previous_edits(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.update(|_, cx| view.update(cx, |view, _| {
        view.add_run(None);
        let previous = view.run_tabs.active;
        view.cell_edit.edits.insert((1, 1), Cell::Text("pending".into()));
        view.add_failed_run("query failed".into(), std::time::Duration::ZERO);
        assert_eq!(view.dataset_count(), 0);
        assert!(view.source.columns().is_empty());
        assert!(!view.export_file_enabled());
        assert!(view.cell_edit.edits.is_empty());
        assert!(!view.run_tabs.slots[previous].edits.is_empty());
        view.activate_run(previous);
        assert_eq!(view.panel.view, bookie_gpui::shell::ResultView::Table);
        assert_eq!(view.cell_at(1, 1), Cell::Text("pending".into()));
    }));
}
