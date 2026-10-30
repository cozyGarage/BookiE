use super::{StreamTerminal, StreamedResult, stream_result_view};

#[test]
fn page_label_lookup_uses_a_constant_number_of_page_index_lookups() {
    let result = StreamedResult {
        pages: Vec::new(),
        page_indices: Vec::new(),
        page_row_counts: vec![1; 10_000],
        page_row_starts: (1..=10_000).collect(),
        retained_rows: 10_000,
        columns: Vec::new(),
        total_pages: 10_000,
        total_rows: 10_000,
        terminal: StreamTerminal::Loading,
        export: None,
        generation: 0,
        spool: None,
    };
    stream_result_view::PAGE_ROW_INDEX_LOOKUPS.with(|visits| visits.set(0));

    let label = stream_result_view::stream_page_label(&result, 9_999);

    let visited = stream_result_view::PAGE_ROW_INDEX_LOOKUPS.with(std::cell::Cell::get);
    assert_eq!(label, "Rows 10000-10000");
    assert!(visited <= 2, "looked up page metadata {visited} times");
}

#[test]
fn progress_text_reads_the_cached_retained_row_count_once() {
    let page = std::sync::Arc::new(tablepro_core::QueryResult {
        columns: Vec::new(),
        rows: vec![Vec::new()],
        truncated: false,
    });
    let result = StreamedResult {
        pages: vec![page; 10_000],
        page_indices: (0..10_000).collect(),
        page_row_counts: vec![1; 10_000],
        page_row_starts: (1..=10_000).collect(),
        retained_rows: 10_000,
        columns: Vec::new(),
        total_pages: 10_000,
        total_rows: 10_000,
        terminal: StreamTerminal::Loading,
        export: None,
        generation: 0,
        spool: None,
    };
    stream_result_view::RETAINED_ROW_COUNT_LOOKUPS.with(|visits| visits.set(0));

    let text = stream_result_view::stream_progress_text(&result);

    let visited = stream_result_view::RETAINED_ROW_COUNT_LOOKUPS.with(std::cell::Cell::get);
    assert!(text.starts_with("Received 10000 rows · 10000 pages"));
    assert_eq!(visited, 1, "read the retained row count {visited} times");
}
