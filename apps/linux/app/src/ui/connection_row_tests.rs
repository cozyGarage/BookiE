use super::*;
use tablepro_core::{Environment, TlsMode};

fn saved(username: &str, auth_mode: AuthMode) -> SavedConnection {
    SavedConnection {
        auth_source: None,
        id: Uuid::new_v4(),
        name: "Corp".into(),
        driver_id: "mssql".into(),
        host: "sql.corp.example".into(),
        port: 1433,
        socket_dir: None,
        socket_path: None,
        database: "sales".into(),
        username: username.into(),
        use_tls: true,
        tls_mode: Some(TlsMode::VerifyFull),
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        auth_mode,
        read_only: false,
        environment: Environment::Prod,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
    }
}

fn plain() -> ConnectionOrganization {
    ConnectionOrganization::default()
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn connection_actions_leave_room_for_text_in_a_narrow_row() {
    adw::init().unwrap();
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .hexpand(true)
        .vexpand(true)
        .build();
    let mut factory = relm4::factory::FactoryVecDeque::<ConnectionRow>::builder()
        .launch(list.clone())
        .detach();
    let mut connection = saved("analyst", AuthMode::Password);
    let title = format!(
        "Production analytics — Billing & audit 東京 {}",
        "Quarterly warehouse metric ".repeat(8)
    );
    connection.name = title.clone();
    connection.host = "primary-warehouse-east-with-a-long-hostname.example".into();
    factory.guard().push_back(ConnectionRowInit {
        saved: connection,
        organization: plain(),
        driver_name: "SQL Server".into(),
    });
    let window = gtk::Window::builder()
        .title("BookiE saved connection preview")
        .default_width(320)
        .default_height(160)
        .child(&list)
        .build();
    window.present();
    let context = gtk::glib::MainContext::default();
    while context.iteration(false) {}
    let provider = gtk::CssProvider::new();
    provider.load_from_string("* { font-size: 20px; }");
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let minimum = list.measure(gtk::Orientation::Horizontal, -1).0;
    window.set_default_size(320, 160);
    while context.iteration(false) {}
    let guard = factory.guard();
    let row = guard.get(0).unwrap();
    let mut pending = vec![list.clone().upcast::<gtk::Widget>()];
    let mut title_width = None;
    let mut title_lines = None;
    let mut subtitle_lines = None;
    while let Some(widget) = pending.pop() {
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            if label.text() == title {
                title_width = Some(label.width());
                title_lines = Some(label.layout().lines().len());
            }
            if label.css_classes().iter().any(|class| class == "subtitle") {
                subtitle_lines = Some(label.layout().lines().len());
            }
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            pending.push(widget);
        }
    }
    if let Some(directory) = std::env::var_os("BOOKIE_CONNECTION_ROW_SCREENSHOT_DIR") {
        capture_connection_row_screenshots(&window, &list, std::path::Path::new(&directory));
    }
    gtk::style_context_remove_provider_for_display(&gtk::gdk::Display::default().unwrap(), &provider);
    assert!(
        minimum <= 320,
        "connection row requires {minimum}px at larger text size"
    );
    assert!(
        title_width.unwrap() >= 120,
        "connection name only receives {title_width:?} pixels"
    );
    assert!(
        title_lines.unwrap() > 1,
        "long connection names should wrap at larger text sizes"
    );
    assert!(
        title_lines.unwrap() <= 2 && subtitle_lines.unwrap() <= 2,
        "connection names and endpoints should stay within two lines"
    );
    assert!(row.favorite_button.width() > 0, "favorite action is not reachable");
    assert!(row.actions_button.width() > 0, "connection menu is not reachable");
    assert!(row.open_button.width() > 0, "open action is not reachable");
}

fn capture_connection_row_screenshots(window: &gtk::Window, list: &gtk::ListBox, directory: &std::path::Path) {
    std::fs::create_dir_all(directory).unwrap();
    let style_manager = adw::StyleManager::default();
    let context = gtk::glib::MainContext::default();
    for (theme, scheme) in [
        ("light", adw::ColorScheme::ForceLight),
        ("dark", adw::ColorScheme::ForceDark),
    ] {
        style_manager.set_color_scheme(scheme);
        for (layout, width) in [("narrow", 320), ("normal", 560)] {
            window.set_default_size(width, 160);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
            while window.width() != width && std::time::Instant::now() < deadline {
                while context.iteration(false) {}
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            while context.iteration(false) {}
            assert_eq!(
                window.width(),
                width,
                "preview window did not reach the requested width"
            );
            let path = directory.join(format!("{theme}-{layout}.png"));
            save_widget_snapshot(window, list, &path);
        }
    }
}

fn save_widget_snapshot(window: &gtk::Window, widget: &gtk::ListBox, path: &std::path::Path) {
    assert!(window.is_mapped(), "preview window is not mapped");
    let paintable = gtk::WidgetPaintable::new(Some(widget));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, f64::from(widget.width()), f64::from(widget.height()));
    let node = snapshot.to_node().unwrap();
    let renderer = window.renderer().unwrap();
    let texture = renderer.render_texture(&node, None);
    texture.save_to_png(path).unwrap();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn connection_menu_dispatches_actions_for_its_own_record() {
    adw::init().unwrap();
    let (sender, _receiver) = relm4::channel::<()>();
    let (observed, outputs) = std::sync::mpsc::channel();
    let mut factory = relm4::factory::FactoryVecDeque::<ConnectionRow>::builder()
        .launch(gtk::ListBox::new())
        .forward(&sender, move |output| observed.send(output).unwrap());
    let connection = saved("analyst", AuthMode::Password);
    let id = connection.id;
    factory.guard().push_back(ConnectionRowInit {
        saved: connection,
        organization: plain(),
        driver_name: "SQL Server".into(),
    });
    let context = gtk::glib::MainContext::default();
    while context.iteration(false) {}
    let button = factory.guard().get(0).unwrap().actions_button.clone();
    let content = button.popover().unwrap().child().unwrap();
    let mut actions = Vec::new();
    let mut child = content.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        actions.push(widget.downcast::<gtk::Button>().unwrap());
    }
    assert_eq!(actions.len(), 4);
    for (expected, action) in actions.iter().take(3).enumerate() {
        action.emit_clicked();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        let output = loop {
            while context.iteration(false) {}
            if let Ok(output) = outputs.try_recv() {
                break output;
            }
            assert!(std::time::Instant::now() < deadline, "connection action did not arrive");
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        match (expected, output) {
            (0, ConnectionRowOutput::Organize(saved)) | (1, ConnectionRowOutput::Edit(saved)) => {
                assert_eq!(saved.id, id);
            }
            (2, ConnectionRowOutput::Duplicate(actual)) => assert_eq!(actual, id),
            (_, other) => panic!("unexpected connection action: {other:?}"),
        }
    }
}

#[test]
fn kerberos_rows_hide_the_username_separator() {
    assert_eq!(
        subtitle_for(&saved("", AuthMode::Kerberos), &plain(), "SQL Server"),
        "Prod · SQL Server · sql.corp.example:1433"
    );
    assert_eq!(
        subtitle_for(&saved("sa", AuthMode::Password), &plain(), "SQL Server"),
        "Prod · SQL Server · sa@sql.corp.example:1433"
    );
}

#[test]
fn saved_connection_subtitle_uses_readable_environment_and_engine_names() {
    let subtitle = subtitle_for(&saved("sa", AuthMode::Password), &plain(), "SQL Server");

    assert!(
        subtitle.starts_with("Prod · SQL Server ·"),
        "saved connection subtitle should lead with readable labels: {subtitle}"
    );
}

#[test]
fn saved_row_uses_the_registered_engine_name_and_falls_back_for_unknown_engines() {
    let registry = crate::build_registry();
    let connection = saved("sa", AuthMode::Password);
    let row = ConnectionRowInit::new(connection.clone(), plain(), &registry);

    assert_eq!(row.driver_name, "SQL Server");

    let mut unknown = connection;
    unknown.driver_id = "future-engine".into();
    let row = ConnectionRowInit::new(unknown, plain(), &registry);

    assert_eq!(row.driver_name, "future-engine");
}

#[test]
fn a_group_and_tags_extend_the_subtitle() {
    let organization = ConnectionOrganization::new(Some("Production"), &["billing".into(), "audit".into()], false)
        .expect("valid organization");
    assert_eq!(
        subtitle_for(&saved("sa", AuthMode::Password), &organization, "SQL Server"),
        "Prod · SQL Server · sa@sql.corp.example:1433 · Production · audit, billing"
    );
}

#[test]
fn an_unorganized_connection_keeps_environment_engine_and_endpoint() {
    assert_eq!(
        subtitle_for(&saved("sa", AuthMode::Password), &plain(), "SQL Server"),
        "Prod · SQL Server · sa@sql.corp.example:1433"
    );
}

#[test]
fn a_palette_colour_adds_its_class_to_the_swatch() {
    let organization = ConnectionOrganization::default().with_color(Some("teal"));
    assert_eq!(color_css_class(&organization), Some("tp-color-teal"));
    assert_eq!(color_classes(&organization), vec!["tp-color-swatch", "tp-color-teal"]);
}

#[test]
fn an_uncoloured_row_carries_only_the_swatch_class() {
    let organization = ConnectionOrganization::default();
    assert_eq!(color_css_class(&organization), None);
    assert_eq!(color_classes(&organization), vec!["tp-color-swatch"]);
}

#[test]
fn a_colour_outside_the_palette_never_reaches_a_css_class() {
    let mut organization = ConnectionOrganization::default();
    organization.color = Some("red; background: url(x)".into());
    assert_eq!(color_css_class(&organization), None);
    assert_eq!(color_classes(&organization), vec!["tp-color-swatch"]);
}

#[test]
fn each_environment_has_its_own_colour_class() {
    assert_eq!(environment_css_class(Environment::Local), "tp-env-local");
    assert_eq!(environment_css_class(Environment::Dev), "tp-env-dev");
    assert_eq!(environment_css_class(Environment::Staging), "tp-env-staging");
    assert_eq!(environment_css_class(Environment::Prod), "tp-env-prod");
}
