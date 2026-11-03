use super::cursor_page;

#[test]
fn streamed_progress_pluralizes_rows_and_pages_independently() {
    for (rows, pages, expected) in [
        (0usize, 0u64, "Complete · Received 0 rows · 0 pages"),
        (1, 1, "Complete · Received 1 row · 1 page"),
        (1, 2, "Complete · Received 1 row · 2 pages"),
        (2, 1, "Complete · Received 2 rows · 1 page"),
        (2, 2, "Complete · Received 2 rows · 2 pages"),
        (215_804, 43, "Complete · Received 215,804 rows · 43 pages"),
    ] {
        let page_row_counts = vec![rows / pages.max(1) as usize; pages as usize];
        let page_row_starts = super::super::super::stream_pages::page_row_starts_from_counts(&page_row_counts);
        let page = std::sync::Arc::new(cursor_page(&vec![1; rows], false).result);
        let retained_rows = page.rows.len();
        let mut result = super::super::super::StreamedResult {
            pages: vec![std::sync::Arc::downgrade(&page)],
            page_indices: vec![pages.saturating_sub(1)],
            page_row_counts,
            page_row_starts,
            retained_rows,
            columns: Vec::new(),
            total_pages: pages,
            total_rows: rows,
            terminal: super::super::super::StreamTerminal::Complete,
            export: None,
            generation: 0,
            spool: None,
            resident_pages: super::super::super::stream_pages::ResidentPageStore::from_pages([(
                pages.saturating_sub(1),
                page,
            )]),
        };
        let text = super::super::stream_result_view::stream_progress_text(&result);
        assert_eq!(text, expected);
        result.terminal = super::super::super::StreamTerminal::Loading;
        assert_eq!(
            super::super::stream_result_view::stream_progress_text(&result),
            expected.replacen("Complete", "Receiving…", 1)
        );
    }
}
