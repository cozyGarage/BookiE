use super::*;
use std::rc::Rc;

fn request() -> ImportRequest {
    ImportRequest {
        schema: None,
        path: PathBuf::from("synthetic.csv"),
        bytes: b"value\n1\n".as_slice().into(),
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

fn wait_until(timeout: std::time::Duration, predicate: impl Fn() -> bool) -> bool {
    let context = gtk::glib::MainContext::default();
    let deadline = std::time::Instant::now() + timeout;
    while !predicate() && std::time::Instant::now() < deadline {
        context.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    predicate()
}

fn large_utf8_csv() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(bookie_core::import::MAX_FILE_BYTES as usize);
    bytes.extend_from_slice(b"person,value\n");
    while bytes.len() + 9 <= bookie_core::import::MAX_FILE_BYTES as usize {
        bytes.extend_from_slice(b"caf\xc3\xa9,42\n");
    }
    bytes
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn import_format_controls_release_their_source_bytes() {
    adw::init().unwrap();
    let request = request();
    let state = Rc::new(State {
        file: FileOptions::new(&request),
        columns: ColumnSection::new(&request),
        skip_rejected: adw::SwitchRow::new(),
        request,
        preview_status: gtk::Label::new(None),
        start_button: gtk::Button::new(),
        preview_queue: std::cell::RefCell::new(super::preview_queue::PreviewQueue::default()),
        ready_generation: std::cell::Cell::new(None),
        auto_detect_encoding: std::cell::Cell::new(true),
        suppress_refresh: std::cell::Cell::new(false),
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
        .application_id("com.bookie.linux.import-lifetime-test")
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

fn find<T: IsA<gtk::Widget>>(root: &gtk::Widget, keep: &dyn Fn(&T) -> bool) -> Option<T> {
    if let Some(found) = root.downcast_ref::<T>().filter(|widget| keep(widget)) {
        return Some(found.clone());
    }
    let mut child = root.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        if let Some(found) = find(&current, keep) {
            return Some(found);
        }
    }
    None
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn the_skip_switch_is_off_by_default_and_its_state_reaches_the_import() {
    adw::init().unwrap();
    let application = adw::Application::builder()
        .application_id("com.bookie.linux.import-skip-switch-test")
        .build();
    application.register(None::<&gtk::gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&application);
    window.present();
    let context = gtk::glib::MainContext::default();

    let mut answers = Vec::new();
    for turn_on in [false, true] {
        let chosen = Rc::new(std::cell::Cell::new(None));
        present(&window, request(), {
            let chosen = chosen.clone();
            move |choice| chosen.set(Some(choice.skip_rejected_rows))
        });
        let dialog = window.visible_dialog().unwrap();
        let root: gtk::Widget = dialog.clone().upcast();
        let switch = find::<adw::SwitchRow>(&root, &|row| row.title() == "Skip rows the database rejects").unwrap();
        assert!(
            !switch.is_active(),
            "rejected rows must not be skipped unless the user asks"
        );
        if turn_on {
            switch.set_active(true);
        }
        let start = find::<gtk::Button>(&root, &|button| button.label().as_deref() == Some("Import")).unwrap();
        assert!(wait_until(std::time::Duration::from_secs(2), || start.is_sensitive()));
        start.emit_clicked();
        while context.iteration(false) {}
        answers.push(chosen.get().expect("the import started"));
    }
    assert_eq!(answers, [false, true]);
    window.close();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn changing_encoding_does_not_block_the_dialog_and_close_discards_its_preview() {
    adw::init().unwrap();
    let application = adw::Application::builder()
        .application_id("com.bookie.linux.csv-preview-responsiveness-test")
        .build();
    application.register(None::<&gtk::gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&application);
    window.present();
    let mut request = request();
    request.bytes = large_utf8_csv().into();
    present(&window, request, |_| {});
    let dialog = window.visible_dialog().unwrap();
    let root: gtk::Widget = dialog.clone().upcast();
    let start = find::<gtk::Button>(&root, &|button| button.label().as_deref() == Some("Import")).unwrap();
    assert!(wait_until(std::time::Duration::from_secs(3), || start.is_sensitive()));
    let encoding = find::<adw::ComboRow>(&root, &|row| row.title() == "Encoding").unwrap();
    let started = std::time::Instant::now();
    encoding.set_selected(1);
    let elapsed = started.elapsed();
    gtk::glib::MainContext::default().iteration(false);
    dialog.close();
    window.close();
    assert!(
        elapsed < std::time::Duration::from_millis(250),
        "encoding change took {elapsed:?}"
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn malformed_preview_displays_an_error_and_keeps_import_disabled() {
    adw::init().unwrap();
    let application = adw::Application::builder()
        .application_id("com.bookie.linux.csv-preview-error-test")
        .build();
    application.register(None::<&gtk::gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&application);
    window.present();
    let mut request = request();
    request.bytes = b"name\n\xff\n".as_slice().into();
    present(&window, request, |_| {});
    let dialog = window.visible_dialog().unwrap();
    let root: gtk::Widget = dialog.clone().upcast();
    let encoding = find::<adw::ComboRow>(&root, &|row| row.title() == "Encoding").unwrap();
    let start = find::<gtk::Button>(&root, &|button| button.label().as_deref() == Some("Import")).unwrap();
    assert!(wait_until(std::time::Duration::from_secs(2), || start.is_sensitive()));
    encoding.set_selected(0);
    assert!(wait_until(std::time::Duration::from_secs(2), || {
        find::<gtk::Label>(&root, &|label| label.has_css_class("error")).is_some()
    }));
    assert!(!start.is_sensitive());
    dialog.close();
    window.close();
}
