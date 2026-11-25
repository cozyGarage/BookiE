use gpui::TestAppContext;

use crate::ui_tests::open;

#[gpui::test]
fn the_connection_form_sets_and_clears_color_with_swatches(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| view.open_connection_dialog(cx));
    cx.run_until_parked();
    let swatch = cx.debug_bounds("swatch-teal").expect("the teal swatch is visible");
    assert!(cx.debug_bounds("teal").is_none(), "the color is presented as a swatch");
    cx.simulate_click(swatch.center(), Default::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.connection_form.color.as_deref(), Some("teal"))
    });

    let clear = cx.debug_bounds("swatch-No color").expect("the clear swatch is visible");
    cx.simulate_click(clear.center(), Default::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_eq!(view.connection_form.color, None));
}

#[gpui::test]
fn tab_moves_between_connection_fields_in_order(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_connection_dialog(cx);
            cx.notify();
            let name = view.connection_form.name.read(cx).focus_handle_for_tests();
            window.focus(&name, cx);
        });
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| {
        assert!(
            view.read(cx)
                .connection_form
                .host
                .read(cx)
                .focus_handle_for_tests()
                .is_focused(window)
        );
    });
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| {
        assert!(
            view.read(cx)
                .connection_form
                .port
                .read(cx)
                .focus_handle_for_tests()
                .is_focused(window)
        );
    });
    cx.simulate_keystrokes("shift-tab");
    cx.update(|window, cx| {
        assert!(
            view.read(cx)
                .connection_form
                .host
                .read(cx)
                .focus_handle_for_tests()
                .is_focused(window)
        );
    });
}

#[gpui::test]
fn connection_sections_use_space_on_wide_windows_and_wrap_when_narrow(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.open_connection_dialog(cx);
        cx.notify();
    });
    for (width, beside) in [(1896.0, true), (1100.0, true), (900.0, false), (720.0, false)] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(760.0)));
        cx.run_until_parked();
        if width > 1500.0 {
            let card = cx.debug_bounds("connection-card").expect("connection form visible");
            assert!(
                card.size.width > gpui::px(1100.0),
                "wide window should use more of its content area"
            );
        }
        let connection = cx
            .debug_bounds("connection-main-section")
            .expect("connection fields visible");
        let options = cx
            .debug_bounds("connection-options-section")
            .expect("connection options visible");
        let page = cx.debug_bounds("connection-page").expect("connection form visible");
        assert!(connection.origin.x + connection.size.width <= page.origin.x + page.size.width);
        assert!(options.origin.x + options.size.width <= page.origin.x + page.size.width);
        if beside {
            assert!(options.origin.x >= connection.origin.x + connection.size.width);
        } else {
            assert!(options.origin.y >= connection.origin.y + connection.size.height);
        }
    }
}

#[gpui::test]
fn connection_form_keeps_actions_visible_at_two_widths(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.open_connection_dialog(cx);
        cx.notify();
    });
    for width in [720.0, 1100.0] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(760.0)));
        cx.run_until_parked();
        let page = cx.debug_bounds("connection-page").expect("connection form is rendered");
        let actions = cx
            .debug_bounds("connection-actions")
            .expect("connection actions are rendered");
        assert!(page.size.width > gpui::px(200.0));
        assert!(actions.origin.y >= page.origin.y);
        assert!(actions.origin.y + actions.size.height <= gpui::px(760.0));
        let card = cx.debug_bounds("connection-card").expect("the form sits on a card");
        assert!(page.origin.y >= card.origin.y && actions.origin.y <= card.origin.y + card.size.height);
        assert!(
            cx.debug_bounds("Prod").is_some(),
            "the environment chips are in the behaviour section"
        );
    }
}

#[gpui::test]
fn editing_a_production_connection_keeps_it_production(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut draft = view.connection_form.draft(cx);
            draft.environment = bookie_core::Environment::Prod;
            view.fill_connection_form(draft, cx);
        })
    });
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.connection_form.draft(cx).environment,
            bookie_core::Environment::Prod
        );
    });
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.connection_form.environment = bookie_core::Environment::Staging
        })
    });
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.connection_form.draft(cx).environment,
            bookie_core::Environment::Staging
        );
    });
}

#[gpui::test]
fn editing_a_coloured_connection_shows_its_colour_and_clearing_drops_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let id = uuid::Uuid::from_u128(3704);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.connections
                .push(bookie_app_services::read_session::ConnectionSummary {
                    id,
                    name: "orders".into(),
                    driver_id: "postgres".into(),
                    read_only: false,
                    environment: bookie_core::Environment::Local,
                    color: Some("teal".into()),
                });
            let mut draft = view.connection_form.draft(cx);
            draft.id = Some(id);
            view.fill_connection_form(draft, cx);
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.connection_form.color.as_deref(), Some("teal"));
        assert_eq!(view.live_color(), None, "no live connection, no colour cue");
    });
    cx.update(|_, cx| view.update(cx, |view, _| view.connection_form.color = None));
    view.read_with(cx, |view, _| assert_eq!(view.connection_form.color, None));
}

#[gpui::test]
fn connection_save_reports_partial_success_and_reuses_the_saved_identity(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let id = uuid::Uuid::from_u128(3895);
    let head = uuid::Uuid::from_u128(3896);
    let jump = uuid::Uuid::from_u128(3897);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.connection_form.ssh.jumps.push(crate::ssh_form::HopForm::new(cx));
            view.finish_connection_save(Ok((id, "orders".into(), vec![head, jump], false)), cx);
            assert_eq!(view.connection_form.editing, Some(id));
            assert_eq!(view.connection_form.ssh.head.hop_id, Some(head));
            assert_eq!(view.connection_form.ssh.jumps[0].hop_id, Some(jump));
            assert_eq!(view.connection_form.draft(cx).id, Some(id));
            assert!(view.connection_form.draft(cx).keep_stored_password);
            assert!(view.connection_form.status.contains("color"));
            view.finish_connection_save(Ok((id, "orders".into(), vec![], true)), cx);
            assert!(!view.connection_form.status.contains("color"));
            view.finish_connection_save(Err("Connection save failed".into()), cx);
            assert!(view.connection_form.status.contains("failed"));
            assert_eq!(view.connection_form.editing, Some(id));
        })
    });
}

fn set(input: &gpui::Entity<crate::text_input::TextInput>, text: &str, cx: &mut gpui::App) {
    input.update(cx, |input, cx| input.set_content(text, cx));
}

#[gpui::test]
fn security_and_ssh_start_collapsed_and_open_from_their_headers(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_resize(gpui::size(gpui::px(1400.0), gpui::px(900.0)));
    view.update(cx, |view, cx| {
        view.open_connection_dialog(cx);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("connection-security-toggle").is_some());
    assert!(cx.debug_bounds("connection-ssh-toggle").is_some());
    assert!(cx.debug_bounds("connection-security-fields").is_none());
    assert!(cx.debug_bounds("connection-ssh-fields").is_none());
    let toggle = cx.debug_bounds("connection-security-toggle").expect("security header");
    cx.simulate_click(toggle.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("connection-security-fields").is_some());
    assert!(cx.debug_bounds("connection-ssh-fields").is_none());
}

#[gpui::test]
fn a_tls_error_inside_a_collapsed_section_opens_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_connection_dialog(cx);
            let form = &view.connection_form;
            for (input, text) in [
                (&form.host, "db.example.com"),
                (&form.port, "5432"),
                (&form.database, "app"),
                (&form.tls_cert, "/etc/ssl/client.pem"),
            ] {
                set(input, text, cx);
            }
            assert!(!view.connection_form.security_open);
            view.save_connection(window, cx);
            assert!(view.connection_form.security_open, "{}", view.connection_form.status);
        });
    });
}

#[gpui::test]
fn a_pasted_url_fills_a_new_connection_and_a_copied_url_has_no_password(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.open_connection_dialog(cx);
            view.fill_from_connection_url(
                &format!(
                    "postgres://alice:{}@db.example.com:5433/sales?sslmode=require",
                    "s3cret"
                ),
                cx,
            );
            let form = &view.connection_form;
            assert_eq!(form.editing, None);
            assert_eq!(form.host.read(cx).content(), "db.example.com");
            assert_eq!(form.port.read(cx).content(), "5433");
            assert_eq!(form.database.read(cx).content(), "sales");
            assert_eq!(form.username.read(cx).content(), "alice");
            assert_eq!(form.password.read(cx).content(), "s3cret");
            assert!(form.security_open, "a URL with TLS settings shows them");
            let copied = view.connection_url_for_form(cx).expect("postgres has a URL");
            assert!(
                copied.starts_with("postgres://alice@db.example.com:5433/sales"),
                "{copied}"
            );
            assert!(!copied.contains("s3cret"), "{copied}");
            view.fill_from_connection_url("not a url", cx);
            assert!(
                view.connection_form
                    .status
                    .starts_with("Could not read the connection URL"),
                "{}",
                view.connection_form.status
            );
        });
    });
}

#[gpui::test]
fn editing_a_connection_with_a_tunnel_opens_the_ssh_section(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let registry = drivers_registry::default_registry();
            let (mut draft, _) =
                bookie_app_services::connection_save::draft_from_url(&registry, "postgres://bob@db.internal:5432/app")
                    .expect("valid url");
            draft.ssh = Some(bookie_app_services::connection_save::SshDraft {
                head: bookie_app_services::connection_save::SshHopDraft {
                    hop_id: None,
                    host: "bastion".into(),
                    port: 22,
                    username: "tunnel".into(),
                    auth: bookie_app_services::connection_save::SshAuthKind::Agent,
                    key_path: String::new(),
                    agent_identity: String::new(),
                    secret: String::new().into(),
                    stored_secret: false,
                },
                openssh: false,
                jumps: Vec::new(),
            });
            view.fill_connection_form(draft, cx);
            assert!(view.connection_form.ssh_open);
            assert!(!view.connection_form.security_open);
        });
    });
}

#[gpui::test]
fn the_toolbar_labels_its_left_actions_when_wide_and_new_connection_opens_the_form(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_resize(gpui::size(gpui::px(1100.0), gpui::px(760.0)));
    cx.run_until_parked();
    let wide = cx
        .debug_bounds("new-connection")
        .expect("the wide toolbar draws the button");
    cx.simulate_resize(gpui::size(gpui::px(700.0), gpui::px(760.0)));
    cx.run_until_parked();
    let narrow = cx
        .debug_bounds("new-connection")
        .expect("the narrow toolbar draws the button");
    assert!(
        wide.size.width > narrow.size.width,
        "labels widen the button at wide sizes"
    );
    cx.simulate_click(narrow.center(), Default::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.connection_form.open);
    });
}

#[gpui::test]
fn new_connection_opens_a_centred_dialog_over_the_current_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_resize(gpui::size(gpui::px(1400.0), gpui::px(800.0)));
    cx.run_until_parked();
    let tabs_before = view.read_with(cx, |view, _| view.tabs.clone());
    let button = cx
        .debug_bounds("new-connection")
        .expect("the toolbar draws New connection");
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.connection_form.open, "the dialog is open");
        assert_eq!(view.tabs, tabs_before, "no tab is opened for the form");
    });
    let dialog = cx.debug_bounds("connection-dialog").expect("the form is a dialog");
    let centre = dialog.origin.x + dialog.size.width / 2.0;
    assert!(
        (centre - gpui::px(700.0)).abs() <= gpui::px(1.0),
        "the dialog is centred: {centre:?}"
    );
    assert!(dialog.origin.y + dialog.size.height <= gpui::px(800.0));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.connection_form.open, "Escape closes the dialog");
        assert_eq!(view.tabs, tabs_before);
    });
    assert!(cx.debug_bounds("connection-dialog").is_none());
}

#[gpui::test]
fn cancel_closes_the_connection_dialog(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.open_connection_dialog(cx);
    });
    cx.run_until_parked();
    let cancel = cx.debug_bounds("cancel-connection").expect("the dialog has Cancel");
    cx.simulate_click(cancel.center(), Default::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(!view.connection_form.open));
}

#[gpui::test]
fn a_successful_save_closes_the_dialog_and_reports_in_the_status_line(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.open_connection_dialog(cx);
        view.finish_connection_save(Ok((uuid::Uuid::new_v4(), "minipc".into(), Vec::new(), true)), cx);
    });
    view.read_with(cx, |view, _| {
        assert!(!view.connection_form.open, "a full save closes the dialog");
        assert_eq!(view.status, "Saved minipc");
    });
    view.update(cx, |view, cx| {
        view.open_connection_dialog(cx);
        view.finish_connection_save(Ok((uuid::Uuid::new_v4(), "minipc".into(), Vec::new(), false)), cx);
    });
    view.read_with(cx, |view, _| {
        assert!(
            view.connection_form.open,
            "a partial save keeps the form so the user can retry"
        );
        assert!(view.connection_form.status.contains("color"));
    });
}

#[gpui::test]
fn the_edit_dialog_names_the_connection_it_edits(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.connection_form.editing = Some(uuid::Uuid::new_v4());
        view.connection_form
            .name
            .update(cx, |input, cx| input.set_content("minipc Redis", cx));
        view.open_connection_dialog(cx);
    });
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.connection_dialog_title(cx), "Edit minipc Redis");
    });
    view.update(cx, |view, cx| {
        view.connection_form
            .name
            .update(cx, |input, cx| input.set_content("", cx));
    });
    view.read_with(cx, |view, cx| {
        assert_eq!(view.connection_dialog_title(cx), "Edit connection")
    });
    view.update(cx, |view, _| view.connection_form.editing = None);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.connection_dialog_title(cx), "New connection")
    });
}
