use super::*;
use std::time::{Duration, Instant};

#[test]
#[ignore = "requires an isolated GTK display"]
fn dismissing_export_setup_releases_its_dialog() {
    adw::init().unwrap();
    let application = adw::Application::builder()
        .application_id("com.bookie.linux.export-lifetime-test")
        .build();
    application.register(None::<&gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&application);
    let toast = adw::ToastOverlay::new();
    window.set_content(Some(&toast));
    window.present();
    present(
        &window,
        &toast,
        ExportRequest {
            scope: ExportScope::TablePage { filtered: false },
            paged: None,
            result: std::sync::Arc::new(QueryResult {
                columns: Vec::new(),
                rows: vec![vec![bookie_core::Value::Bytes(vec![7; 8 * 1024 * 1024])]].into(),
                truncated: false,
            }),
            suggested_name: "synthetic".into(),
            driver_id: "sqlite".into(),
        },
        &PreferencesStore::new(),
    );
    let context = glib::MainContext::default();
    while context.iteration(false) {}
    let dialog = window.visible_dialog().unwrap();
    let retired = dialog.downgrade();
    let button = export_button(dialog.upcast_ref()).unwrap().downgrade();
    dialog.close();
    drop(dialog);
    let deadline = Instant::now() + Duration::from_secs(2);
    while retired.upgrade().is_some() && Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(window.visible_dialog().is_none());
    assert!(retired.upgrade().is_none(), "closed export setup is still retained");
    assert!(button.upgrade().is_none(), "export callback still retains its result");
    window.close();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_running_export_counts_as_busy_work_until_it_finishes() {
    adw::init().unwrap();
    let application = adw::Application::builder()
        .application_id("com.bookie.linux.export-busy-test")
        .build();
    application.register(None::<&gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&application);
    let toast = adw::ToastOverlay::new();
    window.set_content(Some(&toast));
    window.present();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("busy.csv");
    let result = std::sync::Arc::new(QueryResult {
        columns: Vec::new(),
        rows: vec![vec![bookie_core::Value::Bytes(vec![7; 8 * 1024 * 1024])]].into(),
        truncated: false,
    });
    let jobs = bookie_app_services::busy_jobs::global();
    assert_eq!(jobs.active(), 0);

    file::start(
        &window,
        &toast,
        file::ExportJob {
            path: path.clone(),
            source: file::JobSource::Loaded(result),
            format: ExportFormat::Csv,
            options: bookie_core::export::CsvOptions::default(),
            driver_id: "sqlite".into(),
            schema: None,
            table: "t".into(),
        },
    );

    assert_eq!(jobs.active(), 1, "a running export must count as busy work");
    let context = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    while jobs.active() > 0 && Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(jobs.active(), 0, "a finished export must stop counting");
    window.close();
}

fn export_button(widget: &gtk::Widget) -> Option<gtk::Button> {
    if let Some(button) = widget.downcast_ref::<gtk::Button>()
        && button.label().as_deref() == Some("Export…")
    {
        return Some(button.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        if let Some(button) = export_button(&current) {
            return Some(button);
        }
    }
    None
}
