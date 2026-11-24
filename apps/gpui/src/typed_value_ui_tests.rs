use std::sync::Arc;
use std::time::Instant;

use bookie_core::Value;
use bookie_gpui::model::{Cell, ColumnMeta, Diff, FindResult, Predicate};
use bookie_gpui::source::ResultSource;
use gpui::TestAppContext;
use tokio_util::sync::CancellationToken;

use super::{GridView, text_input};

struct TypedSource {
    row: Vec<Value>,
    columns: Vec<ColumnMeta>,
}

impl TypedSource {
    fn new(row: Vec<Value>) -> Self {
        let columns = (0..row.len())
            .map(|index| ColumnMeta {
                name: format!("value_{index}"),
                width: 160.0,
                numeric: false,
                data_type: String::new(),
                primary_key: false,
            })
            .collect();
        Self { row, columns }
    }
}

impl ResultSource for TypedSource {
    fn columns(&self) -> &[ColumnMeta] {
        &self.columns
    }

    fn total(&self) -> u64 {
        1
    }

    fn row(&self, index: u64) -> Vec<Cell> {
        if index == 0 { self.row.clone() } else { Vec::new() }
    }

    fn find(&self, _: &str, _: usize, _: &CancellationToken) -> Result<FindResult, &'static str> {
        Err("not used by this test source")
    }

    fn filter(&self, _: &Predicate, _: &CancellationToken) -> Result<Vec<u32>, &'static str> {
        Err("not used by this test source")
    }

    fn compare(&self, _: bool, _: &CancellationToken) -> Result<Diff, &'static str> {
        Err("not used by this test source")
    }
}

#[gpui::test]
fn selection_and_copy_keep_every_core_value_variant(cx: &mut TestAppContext) {
    use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};

    let values = vec![
        Value::Null,
        Value::Bool(true),
        Value::Int(i64::MAX),
        Value::Float(0.1 + 0.2),
        Value::Text("text α".into()),
        Value::Bytes(vec![0, 255]),
        Value::Date(NaiveDate::from_ymd_opt(9999, 12, 31).expect("date")),
        Value::Time(NaiveTime::from_hms_micro_opt(23, 59, 59, 123456).expect("time")),
        Value::DateTime(
            NaiveDate::from_ymd_opt(9999, 12, 31)
                .expect("date")
                .and_hms_micro_opt(23, 59, 59, 123456)
                .expect("datetime"),
        ),
        Value::TimestampTz(Utc.with_ymd_and_hms(10000, 1, 1, 12, 0, 0).single().expect("timestamp")),
        Value::Decimal("12345678901234567890.12345678".parse().expect("decimal")),
        Value::Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8".parse().expect("UUID")),
        Value::Json(Box::new(serde_json::json!({"n": 1, "text": "α"}))),
        Value::JsonText("{\"duplicate\": 1,  \"duplicate\":2}".into()),
        Value::Undecodable("numeric(40,20)".into()),
    ];
    let source = Arc::new(TypedSource::new(values.clone()));
    let columns = source.columns.clone();
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));

    view.update(cx, |view, cx| {
        view.click_row(0, false);
        assert_eq!(
            view.selected_rows(view.selection.expect("selected row")),
            vec![values.clone()]
        );
        view.copy_selection(false, cx);
    });

    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    let expected = bookie_app_services::result_copy::tsv(
        &bookie_gpui::core_bridge::column_infos_for(&columns),
        vec![values],
        false,
    );
    assert_eq!(copied.as_deref(), Some(expected.as_str()));
}

#[gpui::test]
fn short_headers_keep_room_for_names_and_actions_in_wide_results(cx: &mut TestAppContext) {
    use gpui_kit::test::{TestQueryExt, TestWindowExt};

    let columns = 40;
    let mut source = TypedSource::new(vec![Value::Text("wide".into()); columns]);
    for (index, column) in source.columns.iter_mut().enumerate() {
        column.name = format!("c{:02}", index + 1);
        column.width = 56.0;
    }
    let source = Arc::new(source);
    cx.update(text_input::bind_keys);
    let (view, cx) = cx.add_window_view(|window, cx| GridView::with_source(source, Instant::now(), window, cx));

    for (width, height) in [(1_896.0, 1_150.0), (900.0, 760.0)] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(height)));
        cx.run_until_parked();
        view.update(cx, |view, cx| view.table.update(cx, |table, cx| table.scroll_to_col(0, cx)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
            let name = window.find(("column-name", 0usize));
            assert!(name.visible());
            assert!(name.bounds().size.width >= gpui::px(20.0), "short header has no readable space");
            assert!(window.try_find_by_label("Actions for c01").is_some());
        });

        view.update(cx, |view, cx| view.table.update(cx, |table, cx| table.scroll_to_col(39, cx)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.render_frame(cx);
            window.render_frame(cx);
            let name = window.find(("column-name", 39usize));
            assert!(name.visible(), "last header is not reachable after scrolling");
            assert!(name.bounds().size.width >= gpui::px(20.0), "scrolled short header is not readable");
            assert!(window.try_find_by_label("Actions for c40").is_some());
        });
    }
}
