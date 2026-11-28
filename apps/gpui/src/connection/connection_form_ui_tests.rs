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
fn connection_validation_focuses_the_missing_jump_host_user(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.open_connection_dialog(cx);
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            set(&view.connection_form.host, "db.example.com", cx);
            set(&view.connection_form.port, "5432", cx);
            set(&view.connection_form.database, "app", cx);
            view.connection_form.ssh.on = true;
            set(&view.connection_form.ssh.head.host, "bastion.example.com", cx);
            set(&view.connection_form.ssh.head.user, "alice", cx);
            let jump = crate::ssh_form::HopForm::new(cx);
            set(&jump.host, "jump.example.com", cx);
            set(&jump.user, "", cx);
            view.connection_form.ssh.jumps.push(jump);

            view.save_connection(cx);
        });
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.connection_form.status, "Enter the host and user of jump host 1.");
        assert!(view.connection_form.ssh_open);
    });
    assert!(cx.debug_bounds("connection-ssh-fields").is_some());
    cx.update(|window, cx| {
        let user = view.read(cx).connection_form.ssh.jumps[0]
            .user
            .read(cx)
            .focus_handle_for_tests();
        assert!(user.is_focused(window));
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
fn long_connection_test_status_has_its_own_line_above_the_actions(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.open_connection_dialog(cx);
        view.connection_form.status = "Could not verify the PostgreSQL server certificate. ".repeat(15);
        cx.notify();
    });
    for width in [720.0, 900.0, 1500.0] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(760.0)));
        cx.run_until_parked();
        let status = cx
            .debug_bounds("connection-form-status")
            .expect("the result is visible");
        let test = cx.debug_bounds("test-connection").expect("Test is visible");
        let connect = cx.debug_bounds("connect-connection").expect("Connect is visible");
        assert!(status.size.width > gpui::px(300.0));
        assert!(status.bottom() <= test.top() && status.bottom() <= connect.top());
        assert!(test.bottom() <= gpui::px(760.0) && connect.bottom() <= gpui::px(760.0));
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
            assert_eq!(
                view.finish_connection_save(Ok((id, "orders".into(), vec![head, jump], false)), cx),
                None
            );
            assert_eq!(view.connection_form.editing, Some(id));
            assert_eq!(view.connection_form.ssh.head.hop_id, Some(head));
            assert_eq!(view.connection_form.ssh.jumps[0].hop_id, Some(jump));
            assert_eq!(view.connection_form.draft(cx).id, Some(id));
            assert!(view.connection_form.draft(cx).keep_stored_password);
            assert!(view.connection_form.status.contains("color"));
            assert_eq!(
                view.finish_connection_save(Ok((id, "orders".into(), vec![], true)), cx),
                Some(id)
            );
            assert!(!view.connection_form.status.contains("color"));
            assert_eq!(
                view.finish_connection_save(Err("Connection save failed".into()), cx),
                None
            );
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
    cx.update(|_, cx| {
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
            view.save_connection(cx);
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
fn new_connection_clears_fields_left_from_the_previous_edit(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let previous = uuid::Uuid::from_u128(4219);
    let default_port = view.read_with(cx, |view, _| {
        view.connection_form
            .drivers
            .iter()
            .find(|driver| driver.id == "postgres")
            .map(|driver| driver.port.to_string())
            .unwrap_or_default()
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.connection_form.editing = Some(previous);
            view.connection_form.read_only = false;
            view.connection_form.environment = bookie_core::Environment::Prod;
            view.connection_form.color = Some("teal".into());
            view.connection_form.kerberos = true;
            view.connection_form.tls = bookie_core::TlsMode::Require.index();
            view.connection_form.security_open = true;
            view.connection_form.ssh_open = true;
            view.connection_form.ssh.on = true;
            view.connection_form.ssh.openssh = true;
            view.connection_form.ssh.jumps.push(crate::ssh_form::HopForm::new(cx));
            set(&view.connection_form.name, "prod-db", cx);
            set(&view.connection_form.host, "db.example.com", cx);
            set(&view.connection_form.port, "3306", cx);
            set(&view.connection_form.database, "app", cx);
            set(&view.connection_form.username, "alice", cx);
            set(&view.connection_form.password, "secret", cx);
            set(&view.connection_form.tls_root, "/tmp/ca.pem", cx);
            set(&view.connection_form.tls_cert, "/tmp/client.pem", cx);
            set(&view.connection_form.tls_key, "/tmp/client.key", cx);
            set(&view.connection_form.socket, "/var/run/mysqld", cx);
            set(&view.connection_form.ssh.head.host, "bastion.example.com", cx);
            set(&view.connection_form.ssh.head.user, "jump", cx);
            set(&view.connection_form.ssh.head.secret, "hop-secret", cx);
            view.run_palette_id("new-connection", window, cx);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        let draft = view.connection_form.draft(cx);
        assert!(view.connection_form.open);
        assert_eq!(view.connection_form.editing, None);
        assert!(view.connection_form.read_only);
        assert_eq!(view.connection_form.environment, bookie_core::Environment::default());
        assert_eq!(view.connection_form.color, None);
        assert!(!view.connection_form.kerberos);
        assert_eq!(view.connection_form.tls, bookie_core::TlsMode::default().index());
        assert!(!view.connection_form.security_open);
        assert!(!view.connection_form.ssh_open);
        assert!(!view.connection_form.ssh.on);
        assert!(!view.connection_form.ssh.openssh);
        assert!(view.connection_form.ssh.jumps.is_empty());
        assert_eq!(draft.name, "");
        assert_eq!(draft.host, "");
        assert_eq!(draft.port.to_string(), default_port);
        assert_eq!(draft.database, "");
        assert_eq!(draft.username, "");
        assert!(secrecy::ExposeSecret::expose_secret(&draft.password).is_empty());
        assert_eq!(draft.tls_root_cert, "");
        assert_eq!(draft.tls_client_cert, "");
        assert_eq!(draft.tls_client_key, "");
        assert_eq!(draft.socket_endpoint, "");
        assert!(draft.ssh.is_none());
        assert_eq!(view.connection_form.ssh.head.host.read(cx).content(), "");
        assert_eq!(view.connection_form.ssh.head.user.read(cx).content(), "");
        assert_eq!(view.connection_form.ssh.head.secret.read(cx).content(), "");
        assert_eq!(
            view.connection_form.selected().map(|driver| driver.id),
            Some("postgres")
        );
    });
}

#[gpui::test]
fn a_late_save_result_does_not_close_or_fill_a_new_connection_form(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let request_generation = view.read_with(cx, |view, _| view.connection_form.request_generation);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.connection_form.saving = true;
            view.reset_connection_form(cx);
            view.open_connection_dialog(cx);
            view.finish_connection_save_request(
                request_generation,
                Ok((uuid::Uuid::from_u128(4223), "old profile".into(), Vec::new(), true)),
                cx,
            );
        })
    });
    view.read_with(cx, |view, cx| {
        assert!(view.connection_form.open);
        assert_eq!(view.connection_form.editing, None);
        assert_eq!(view.connection_form.status, "");
        assert_eq!(view.connection_form.draft(cx).name, "");
    });
}

#[gpui::test]
fn a_late_test_result_does_not_replace_the_new_connection_status(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let request_generation = view.read_with(cx, |view, _| view.connection_form.request_generation);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.connection_form.testing = true;
            view.reset_connection_form(cx);
            view.open_connection_dialog(cx);
            view.finish_connection_test_request(request_generation, Ok("Connected".into()), cx);
        })
    });
    view.read_with(cx, |view, _| {
        assert!(view.connection_form.open);
        assert!(!view.connection_form.testing);
        assert_eq!(view.connection_form.status, "");
    });
}

#[gpui::test]
fn a_successful_delete_clears_the_form_so_save_cannot_reuse_stored_secrets(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let deleted = uuid::Uuid::from_u128(4222);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.connection_form.open = true;
            view.connection_form.editing = Some(deleted);
            view.connection_form.read_only = false;
            view.connection_form.ssh_open = true;
            view.connection_form.ssh.on = true;
            view.connection_form.ssh.head.stored = true;
            view.connection_form.ssh.jumps.push(crate::ssh_form::HopForm::new(cx));
            view.connection_form.ssh.jumps[0].stored = true;
            set(&view.connection_form.name, "prod-db", cx);
            set(&view.connection_form.host, "db.example.com", cx);
            set(&view.connection_form.username, "alice", cx);
            set(&view.connection_form.ssh.head.host, "bastion.example.com", cx);
            set(&view.connection_form.ssh.head.user, "jump", cx);
            view.finish_delete(
                Ok(bookie_app_services::connection_save::Deleted {
                    credential_failure: false,
                }),
                cx,
            );
        })
    });
    view.read_with(cx, |view, cx| {
        let draft = view.connection_form.draft(cx);
        assert_eq!(view.connection_form.editing, None);
        assert_eq!(view.connection_form.status, "Deleted");
        assert_eq!(draft.host, "");
        assert_eq!(draft.username, "");
        assert!(draft.ssh.is_none());
        assert!(!view.connection_form.ssh.on);
        assert!(!view.connection_form.ssh.head.stored);
        assert!(view.connection_form.ssh.jumps.is_empty());
        assert_eq!(view.connection_form.ssh.head.host.read(cx).content(), "");
        assert_eq!(view.connection_form.ssh.head.user.read(cx).content(), "");
    });
}

#[gpui::test]
fn a_missing_connection_draft_does_not_open_the_edit_dialog(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let previous = uuid::Uuid::from_u128(4220);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.connection_form.editing = Some(previous);
            set(&view.connection_form.host, "db.example.com", cx);
            set(&view.connection_form.username, "alice", cx);
            view.edit_connection(uuid::Uuid::from_u128(4221), cx);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert!(!view.connection_form.open, "a failed load must not open Save");
        assert_eq!(view.status, "That connection no longer exists");
        assert_eq!(view.connection_form.editing, Some(previous));
        assert_eq!(view.connection_form.host.read(cx).content(), "db.example.com");
        assert_eq!(view.connection_form.username.read(cx).content(), "alice");
    });
}

#[gpui::test]
fn the_toolbar_keeps_its_left_actions_icon_only_and_new_connection_opens_the_form(cx: &mut TestAppContext) {
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
    assert_eq!(wide.size, narrow.size, "secondary toolbar actions stay icon-only");
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
fn connect_in_the_new_connection_form_validates_before_saving(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| view.open_connection_dialog(cx));
    cx.run_until_parked();
    let connect = cx.debug_bounds("connect-connection").expect("the dialog has Connect");
    cx.simulate_click(connect.center(), Default::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.connection_form.open);
        assert!(!view.connection_form.status.is_empty());
        assert!(view.live.is_none());
    });
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
        let id = uuid::Uuid::new_v4();
        assert_eq!(
            view.finish_connection_save(Ok((id, "minipc".into(), Vec::new(), true)), cx),
            Some(id)
        );
    });
    view.read_with(cx, |view, _| {
        assert!(!view.connection_form.open, "a full save closes the dialog");
        assert_eq!(view.status, "Saved minipc");
    });
    view.update(cx, |view, cx| {
        view.open_connection_dialog(cx);
        assert_eq!(
            view.finish_connection_save(Ok((uuid::Uuid::new_v4(), "minipc".into(), Vec::new(), false)), cx),
            None
        );
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

#[gpui::test]
fn deleting_a_connection_asks_in_a_dialog_that_names_it_and_has_no_default(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.connection_form.editing = Some(uuid::Uuid::new_v4());
        view.connection_form
            .name
            .update(cx, |input, cx| input.set_content("minipc Redis", cx));
        view.open_connection_dialog(cx);
    });
    cx.run_until_parked();
    let delete = cx
        .debug_bounds("delete-connection")
        .expect("the edit form offers Delete");
    cx.simulate_click(delete.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("delete-connection-dialog").is_some());
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.delete_connection_question(cx).as_deref(),
            Some("Delete the connection minipc Redis?")
        );
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.connection_form.confirm_delete, "enter does not delete");
        assert!(view.connection_form.editing.is_some());
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.connection_form.confirm_delete);
        assert!(view.connection_form.open, "escape closes only the question");
        assert!(view.connection_form.editing.is_some());
    });
    assert!(cx.debug_bounds("delete-connection-dialog").is_none());
}
