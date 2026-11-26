use bookie_app_services::result_chart::{Aggregate, Kind};
use bookie_gpui::shell::ResultView;
use gpui::TestAppContext;

use crate::live_ui_tests::open_live;
use crate::ui_tests::test_kit::until;

#[gpui::test]
fn a_result_charts_its_totals_and_keeps_the_latest_picks_when_reopened(cx: &mut TestAppContext) {
    let (view, cx, _orders) = open_live(cx);
    let sql = "SELECT customer, total FROM orders ORDER BY customer";
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_query();
            view.sync_grid_owner();
            view.sql_input.update(cx, |input, cx| input.set_value(sql, window, cx));
            view.run_query(cx);
        })
    });
    until(&view, cx, "the chart settles", |view| {
        view.source.executed_sql() == Some(sql)
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.show_chart(cx)));
    until(&view, cx, "the chart settles", |view| {
        view.panel.chart.is_some() && !view.panel.chart_loading
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.view, ResultView::Chart);
        let chart = view.panel.chart.as_ref().expect("a chart");
        assert_eq!(chart.settings.kind, Kind::Bars);
        assert_eq!(chart.columns[chart.settings.label], "customer");
        assert_eq!(chart.settings.slot_of(1), Some(0));
        assert!(chart.data.labels.iter().any(|label| label.as_deref() == Some("Ada")));
        assert!(chart.data.combines());
        assert_eq!(chart.settings.aggregate, Aggregate::Sum);
    });
    assert!(cx.debug_bounds("chart-plot").is_some(), "the plot is drawn");
    assert!(
        cx.debug_bounds("chart-coverage").is_some(),
        "the retained scope is visible"
    );
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.edit_chart(|settings| settings.aggregate = Aggregate::Average, cx);
            view.edit_chart(|settings| settings.aggregate = Aggregate::Count, cx);
            view.invalidate_scans();
        })
    });
    until(&view, cx, "the chart settles", |view| !view.panel.chart_loading);
    let rows = view.read_with(cx, |view, _| {
        std::sync::Arc::downgrade(&view.panel.chart.as_ref().expect("chart").rows)
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.panel.view = ResultView::Table;
            view.show_chart(cx);
        })
    });
    view.read_with(cx, |view, _| {
        let chart = view.panel.chart.as_ref().expect("cached chart");
        assert!(std::sync::Weak::ptr_eq(&rows, &std::sync::Arc::downgrade(&chart.rows)));
        assert_eq!(chart.settings.aggregate, Aggregate::Count);
        assert_eq!(chart.settings.slot_of(1), Some(0));
        let ada = chart
            .data
            .labels
            .iter()
            .position(|label| label.as_deref() == Some("Ada"))
            .expect("Ada");
        assert_eq!(chart.data.series[0].values[ada], Some(2.0));
    });
}

#[gpui::test]
fn replacing_and_closing_results_release_their_chart_rows_and_data(cx: &mut TestAppContext) {
    let (view, cx, _orders) = open_live(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let table = view.source.clone();
            view.tabs.open_query();
            view.sync_grid_owner();
            view.add_run(Some(table));
        })
    });
    for closing in [false, true] {
        cx.update(|_, cx| view.update(cx, |view, cx| view.show_chart(cx)));
        until(&view, cx, "the chart settles", |view| {
            view.panel.chart.is_some() && !view.panel.chart_loading
        });
        let (rows, data) = view.read_with(cx, |view, _| {
            let chart = view.panel.chart.as_ref().expect("chart");
            (
                std::sync::Arc::downgrade(&chart.rows),
                std::sync::Arc::downgrade(&chart.data),
            )
        });
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                if closing {
                    view.close_run(view.run_tabs.active);
                } else {
                    view.add_run(Some(view.source.clone()));
                }
                assert!(view.panel.chart.is_none());
                assert!(!view.panel.chart_loading);
                assert_ne!(view.panel.view, ResultView::Chart);
            })
        });
        assert!(rows.upgrade().is_none());
        assert!(data.upgrade().is_none());
    }
}

#[gpui::test]
fn a_late_chart_preparation_or_recalculation_cannot_restore_a_replaced_result(cx: &mut TestAppContext) {
    let (view, cx, _orders) = open_live(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.add_run(Some(view.source.clone()));
            view.show_chart(cx);
            view.invalidate_background();
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.panel.chart.is_none());
        assert!(!view.panel.chart_loading);
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.show_chart(cx)));
    until(&view, cx, "the chart settles", |view| {
        view.panel.chart.is_some() && !view.panel.chart_loading
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.edit_chart(|settings| settings.aggregate = Aggregate::Average, cx);
            view.close_run(view.run_tabs.active);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.panel.chart.is_none());
        assert!(!view.panel.chart_loading);
    });
}

#[test]
fn dropping_a_work_ticket_cancels_its_pending_chart_work() {
    let cancel = {
        let ticket = crate::Ticket::new();
        ticket.cancel.clone()
    };
    assert!(cancel.is_cancelled());
}

#[gpui::test]
fn a_wide_result_keeps_the_number_controls_bounded(cx: &mut TestAppContext) {
    let (view, cx, _orders) = open_live(cx);
    let sql = format!(
        "SELECT {}",
        (0..512)
            .map(|index| format!("1 AS n{index}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_query();
            view.sync_grid_owner();
            view.sql_input.update(cx, |input, cx| input.set_value(&sql, window, cx));
            view.run_query(cx);
        })
    });
    until(&view, cx, "the chart settles", |view| {
        view.source.executed_sql() == Some(sql.as_str())
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.show_chart(cx)));
    until(&view, cx, "the chart settles", |view| {
        view.panel.chart.is_some() && !view.panel.chart_loading
    });
    let built = [
        "chart-value-0",
        "chart-value-1",
        "chart-value-2",
        "chart-value-3",
        "chart-value-4",
        "chart-value-5",
        "chart-value-511",
    ]
    .into_iter()
    .filter(|selector| cx.debug_bounds(selector).is_some())
    .count();
    assert!(
        built <= bookie_app_services::result_chart::SLOTS,
        "built {built} number controls"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_picker(crate::palette_state::PaletteMode::ChartValues, window, cx)
        })
    });
    view.read_with(cx, |view, cx| assert_eq!(view.picker_items(cx).len(), 512));
    cx.simulate_input("n300");
    view.read_with(cx, |view, cx| {
        let items = view.picker_items(cx);
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0].pick, crate::picker::Pick::ChartValue(300)));
    });
    cx.simulate_keystrokes("enter");
    until(&view, cx, "the chart settles", |view| !view.panel.chart_loading);
    view.read_with(cx, |view, _| {
        assert!(
            view.panel
                .chart
                .as_ref()
                .expect("chart")
                .settings
                .slot_of(300)
                .is_some()
        )
    });
}
