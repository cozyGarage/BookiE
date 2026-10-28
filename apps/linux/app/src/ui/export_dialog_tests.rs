use super::*;

#[test]
fn loaded_export_keeps_the_existing_row_allocation() {
    let result = std::sync::Arc::new(QueryResult {
        columns: Vec::new(),
        rows: vec![vec![tablepro_core::Value::Text("exact export value".into())]],
        truncated: true,
    });
    let file::JobSource::Loaded(loaded) = job_source(&None, &result) else {
        panic!("expected loaded export");
    };
    assert_eq!(loaded.rows.as_ptr(), result.rows.as_ptr());
    assert_eq!(loaded.rows, result.rows);
    assert!(loaded.truncated);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_full_export_shows_a_refusal_without_opening_its_source() {
    adw::init().unwrap();
    let application = adw::Application::builder()
        .application_id("com.tablepro.linux.export-test")
        .build();
    application.register(None::<&gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&application);
    let toast = adw::ToastOverlay::new();
    window.set_content(Some(&toast));
    window.present();
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let opened = calls.clone();
    let request = ExportRequest {
        scope: ExportScope::TablePage { filtered: false },
        paged: Some(PagedExport {
            mode: PagedExportMode::FullTable,
            total_rows: Some(1),
            open: std::sync::Arc::new(move |_| {
                opened.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                PageFetcher::new(|| Ok(None), || Ok(()))
            }),
        }),
        result: QueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            truncated: false,
        },
        suggested_name: "table.csv".into(),
        driver_id: "postgres".into(),
    };
    present_with_format(&window, &toast, request, false, &PreferencesStore::new());
    while glib::MainContext::default().iteration(false) {}
    let dialog = window.visible_dialog().unwrap();
    assert!(dialog.is::<adw::AlertDialog>());
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    dialog.close();
    window.close();
}

#[test]
fn a_qualified_result_name_becomes_a_schema_and_table_for_sql_statements() {
    assert_eq!(
        insert_target("public.customers"),
        (Some("public".into()), "customers".into())
    );
    assert_eq!(insert_target("customers"), (None, "customers".into()));
    assert_eq!(insert_target("Query 1"), (None, "Query 1".into()));
    assert_eq!(insert_target(".customers"), (None, ".customers".into()));
}

#[test]
fn a_result_with_no_source_table_offers_its_label_for_the_user_to_correct() {
    assert_eq!(insert_target("query-results"), (None, "query-results".into()));
    assert_eq!(
        insert_target("orders"),
        (None, "orders".into()),
        "an edited table name is used verbatim rather than the result label"
    );
}

#[test]
fn a_connection_without_sql_literals_is_not_offered_the_statement_format() {
    assert!(available_formats("postgres", None).contains(&ExportFormat::Sql));
    for driver_id in ["mongodb", "redis"] {
        assert!(
            !available_formats(driver_id, None).contains(&ExportFormat::Sql),
            "{driver_id}"
        );
        assert_eq!(available_formats(driver_id, None).len(), FORMATS.len() - 1);
    }
}

#[test]
fn the_csv_options_group_belongs_to_csv_alone() {
    for spec in &FORMATS {
        assert_eq!(
            spec.format.shows_csv_options(),
            spec.format == ExportFormat::Csv,
            "{:?}",
            spec.format
        );
    }
}

#[test]
fn the_selected_row_maps_to_the_format_offered_at_that_position() {
    let formats = available_formats("mongodb", None);
    assert_eq!(selected_format(&formats, 0), ExportFormat::Csv);
    assert_eq!(selected_format(&formats, 5), ExportFormat::Xlsx);
    assert_eq!(selected_format(&formats, 99), ExportFormat::Csv);
    assert_eq!(
        selected_format(&available_formats("postgres", None), 5),
        ExportFormat::Sql
    );
}

#[test]
fn every_offered_format_has_its_own_extension_and_media_type() {
    for spec in &FORMATS {
        assert_eq!(spec.format.extension(), spec.extension);
        assert_eq!(spec.format.mime_type(), spec.mime_type);
        assert_eq!(
            FORMATS.iter().filter(|other| other.extension == spec.extension).count(),
            1,
            "{}",
            spec.extension
        );
    }
    assert_eq!(
        suggested_file_name("customers.csv", ExportFormat::Xlsx),
        "customers.xlsx"
    );
    assert_eq!(suggested_file_name("customers.md", ExportFormat::Sql), "customers.sql");
}

#[test]
fn suggested_file_name_uses_the_selected_format() {
    assert_eq!(suggested_file_name("customers", ExportFormat::Csv), "customers.csv");
    assert_eq!(
        suggested_file_name("customers.csv", ExportFormat::Json),
        "customers.json"
    );
    assert_eq!(
        suggested_file_name("customers.json", ExportFormat::Csv),
        "customers.csv"
    );
}

#[test]
fn a_file_chooser_duplicate_extension_is_removed_once() {
    assert_eq!(
        normalize_duplicate_extension(std::path::PathBuf::from("/tmp/report.sql.sql"), "sql"),
        std::path::PathBuf::from("/tmp/report.sql")
    );
    assert_eq!(
        normalize_duplicate_extension(std::path::PathBuf::from("/tmp/report.csv.csv"), "csv"),
        std::path::PathBuf::from("/tmp/report.csv")
    );
    assert_eq!(
        normalize_duplicate_extension(std::path::PathBuf::from("/tmp/report.txt.sql"), "sql"),
        std::path::PathBuf::from("/tmp/report.txt.sql")
    );
}

#[test]
fn a_workbook_needs_a_known_row_count_when_rows_are_read_page_by_page() {
    let unknown = PagedExport {
        mode: PagedExportMode::FullTable,
        total_rows: None,
        open: std::sync::Arc::new(|_| PageFetcher::new(|| Ok(None), || Ok(()))),
    };
    let known = PagedExport {
        mode: PagedExportMode::FullTable,
        total_rows: Some(10),
        open: unknown.open.clone(),
    };
    assert!(!available_formats("sqlite", Some(&unknown)).contains(&ExportFormat::Xlsx));
    assert!(available_formats("sqlite", Some(&known)).contains(&ExportFormat::Xlsx));
    assert!(available_formats("sqlite", None).contains(&ExportFormat::Xlsx));
}
