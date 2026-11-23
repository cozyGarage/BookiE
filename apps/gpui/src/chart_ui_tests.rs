use bookie_app_services::result_chart::{Aggregate, Kind};
use bookie_gpui::shell::ResultView;
use gpui::TestAppContext;

use crate::live_ui_tests::open_live;

#[gpui::test]
fn a_result_charts_its_totals_per_customer_and_keeps_the_picks(cx: &mut TestAppContext) {
    let (view, cx) = open_live(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.tabs.open_query();
            view.sync_grid_owner();
            view.sql_input.update(cx, |input, cx| {
                input.set_value("SELECT customer, total FROM orders ORDER BY customer", window, cx)
            });
            view.run_query(cx);
        })
    });
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|_, cx| view.update(cx, |view, _| view.show_chart()));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.view, ResultView::Chart);
        let chart = view.panel.chart.as_ref().expect("a chart");
        assert_eq!(chart.settings.kind, Kind::Bars);
        assert_eq!(chart.columns[chart.settings.label], "customer");
        assert_eq!(chart.settings.slot_of(1), Some(0), "total is the first value column");
        let labels: Vec<_> = chart.data.labels.iter().flatten().cloned().collect();
        assert!(labels.contains(&"Ada".to_string()), "{labels:?}");
        assert!(chart.data.combines(), "Ada has two orders, so rows share a label");
        assert_eq!(chart.settings.aggregate, Aggregate::Sum);
    });
    assert!(cx.debug_bounds("chart-plot").is_some(), "the plot is drawn");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.panel.view = ResultView::Table;
            view.panel.view = ResultView::Chart;
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.chart.as_ref().map(|chart| chart.settings.slot_of(1)), Some(Some(0)));
    });
}
