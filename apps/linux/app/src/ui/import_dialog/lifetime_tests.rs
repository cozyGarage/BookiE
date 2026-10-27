use super::*;
use std::rc::Rc;

fn request() -> ImportRequest {
    ImportRequest {
        schema: None,
        path: PathBuf::from("synthetic.csv"),
        bytes: b"value\n1\n".to_vec(),
        format: CsvFormat {
            delimiter: CsvDelimiter::Comma,
            has_header: true,
        },
        mode: ImportMode::NewTable {
            driver_id: "sqlite".into(),
            capabilities: DriverCapabilities::default(),
            suggested_table: "items".into(),
        },
        mapping_store: None,
        mapping_scope: None,
    }
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn import_format_controls_release_their_source_bytes() {
    adw::init().unwrap();
    let request = request();
    let state = Rc::new(State {
        file: FileOptions::new(&request),
        columns: ColumnSection::new(&request),
        request,
    });
    connect_refresh(&state);
    let retired = Rc::downgrade(&state);
    drop(state);
    assert!(retired.upgrade().is_none(), "format callbacks retain the import source");
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn dismissing_import_setup_releases_its_callback() {
    adw::init().unwrap();
    let application = adw::Application::builder()
        .application_id("com.tablepro.linux.import-lifetime-test")
        .build();
    application.register(None::<&gtk::gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&application);
    window.present();
    let callback = Rc::new(());
    let retired = Rc::downgrade(&callback);
    present(&window, request(), move |_| {
        let _ = &callback;
    });
    let dialog = window.visible_dialog().unwrap();
    dialog.close();
    drop(dialog);
    let context = gtk::glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while retired.upgrade().is_some() && std::time::Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(retired.upgrade().is_none(), "closed import setup retains its callback");
    window.close();
}
