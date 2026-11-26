use gpui::TestAppContext;

use crate::ui_tests::test_kit::{NARROW, WIDE};

#[gpui::test]
fn hiding_results_cancels_explain_and_drops_its_late_result(cx: &mut TestAppContext) {
    let (view, cx) = crate::ui_tests::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.begin_background(
                crate::Work::Query,
                cx,
                |_| (),
                |view, ()| view.status = "stale plan".into(),
            );
            view.panel.explaining = true;
            let cancel = view.queries.cancel.clone();
            view.toggle_results();
            assert!(cancel.is_cancelled());
            assert!(!view.panel.explaining);
            assert_eq!(view.status, "Canceled the explain");
            assert!(!view.panel.open);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_eq!(view.status, "Canceled the explain"));
}

#[gpui::test]
fn format_rewrites_the_editor_with_the_shared_formatter_and_leaves_empty_text_alone(cx: &mut TestAppContext) {
    let (view, cx, _orders) = crate::live_ui_tests::open_live(cx);
    let original = "select id, customer from orders where total > 1 order by id";
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_query();
            view.sync_grid_owner();
            view.sql_input
                .update(cx, |input, cx| input.set_value(original, window, cx));
            view.format_query(window, cx);
        })
    });
    let grammar = bookie_app_services::result_editability::grammar_for_driver("sqlite").expect("sqlite grammar");
    let expected = bookie_core::sql_format::format_script(
        original,
        grammar,
        bookie_core::sql_syntax::script::LexicalSettings::default_for(grammar),
    );
    view.read_with(cx, |view, cx| {
        assert_eq!(view.sql_input.read(cx).value().to_string(), expected);
        assert_ne!(expected, original, "the formatter changes this statement");
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.sql_input.update(cx, |input, cx| input.set_value("  ", window, cx));
            view.status.clear();
            view.format_query(window, cx);
            assert!(view.status.is_empty(), "blank text is not formatted");
        })
    });
}

#[gpui::test]
fn a_blocked_explain_cannot_cancel_the_query_already_running(cx: &mut TestAppContext) {
    let (view, cx, _orders) = crate::live_ui_tests::open_live(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.sql_input
                .update(cx, |input, cx| input.set_value("SELECT id FROM orders", window, cx));
            view.run_query(cx);
            let query = view.queries.cancel.clone();
            view.save.busy = true;
            view.explain_query(cx);
            assert!(view.status.contains("save is running"));
            view.toggle_results();
            assert!(!query.is_cancelled());
            assert!(!view.panel.explaining);
        });
    });
}

#[gpui::test]
fn replacement_load_does_not_inherit_explain_cancellation(cx: &mut TestAppContext) {
    let (view, cx, _orders) = crate::live_ui_tests::open_live(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.sql_input
                .update(cx, |input, cx| input.set_value("SELECT id FROM orders", window, cx));
            view.explain_query(cx);
            assert!(view.panel.explaining);
            let explain = view.queries.cancel.clone();
            view.run_query(cx);
            let query = view.queries.cancel.clone();
            view.toggle_results();
            assert!(explain.is_cancelled());
            assert!(!query.is_cancelled());
            assert!(!view.panel.explaining);
        });
    });
}

#[gpui::test]
fn selecting_an_older_result_leaves_the_query_running(cx: &mut TestAppContext) {
    let (view, cx, _orders) = crate::live_ui_tests::open_live(cx);
    let selected = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_query();
            view.sync_grid_owner();
            view.add_run(None);
            view.add_run(None);
            assert_ne!(view.run_tabs.active, 0);
            view.sql_input
                .update(cx, |input, cx| input.set_value("SELECT 7", window, cx));
            view.run_query(cx);
            view.select_run(0);
            assert!(!view.queries.cancel.is_cancelled());
            view.run_tabs.slots[0].run.number
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        use bookie_gpui::model::Cell;
        let tab = &view.run_tabs.slots[view.run_tabs.active];
        assert_eq!(tab.run.number, selected);
        let latest = view.run_tabs.slots.last().unwrap().source.as_ref().unwrap();
        assert_eq!(latest.row(0), [Cell::Int(7)]);
    });
}

#[gpui::test]
fn a_query_can_show_its_result_after_the_last_older_result_is_closed(cx: &mut TestAppContext) {
    let (view, cx, _orders) = crate::live_ui_tests::open_live(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_query();
            view.sync_grid_owner();
            view.add_run(None);
            view.sql_input
                .update(cx, |input, cx| input.set_value("SELECT 7", window, cx));
            view.run_query(cx);
            while !view.run_tabs.slots.is_empty() {
                view.close_run(0);
            }
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.source.row(0), [bookie_gpui::model::Cell::Int(7)])
    });
}

#[gpui::test]
fn a_query_finishing_behind_another_result_replaces_the_running_status(cx: &mut TestAppContext) {
    let (view, cx, _orders) = crate::live_ui_tests::open_live(cx);
    for (sql, failed) in [("SELECT 7", false), ("SELECT nope FROM orders", true)] {
        let running = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.tabs.open_query();
                view.sync_grid_owner();
                view.add_run(None);
                view.add_run(None);
                view.sql_input.update(cx, |input, cx| input.set_value(sql, window, cx));
                view.run_query(cx);
                let running = view.status.clone();
                view.select_run(0);
                running
            })
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_ne!(view.status, running, "{sql} left the running status");
            assert!(!view.status.is_empty());
            assert_eq!(view.status.contains("nope"), failed, "{}", view.status);
        });
    }
}

#[gpui::test]
fn the_query_toolbar_keeps_its_tools_in_place_while_a_query_runs(cx: &mut TestAppContext) {
    use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
    let (view, cx) = crate::ui_tests::open(cx);
    cx.simulate_keystrokes("ctrl-t");
    cx.run_until_parked();
    for (width, tools) in [(NARROW, "Editor tools"), (WIDE, "Tools")] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(800.0)));
        let mut positions = Vec::new();
        for (running, selected) in [(false, false), (true, false), (false, true)] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.panel.running = running.then(|| (view.queries.generation, std::time::Instant::now()));
                    view.sql_input.update(cx, |input, cx| {
                        input.set_value("SELECT 1", window, cx);
                        if selected {
                            input.select_all(window, cx);
                        }
                    });
                    cx.notify();
                })
            });
            cx.run_until_parked();
            positions.push(cx.update(|window, cx| {
                window.render_frame(cx);
                window.find_by_label(tools).bounds().origin.x
            }));
        }
        assert!(
            positions.iter().all(|x| *x == positions[0]),
            "{tools} moved at {width}px: {positions:?}"
        );
    }
}
