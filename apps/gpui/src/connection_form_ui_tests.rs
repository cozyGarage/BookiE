use gpui::TestAppContext;

use crate::ui_tests::open;

#[gpui::test]
fn connection_form_keeps_actions_visible_at_two_widths(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        view.tabs.open_new_connection();
        cx.notify();
    });
    for width in [720.0, 1100.0] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(760.0)));
        cx.run_until_parked();
        let page = cx.debug_bounds("connection-page").expect("connection form is rendered");
        let actions = cx.debug_bounds("connection-actions").expect("connection actions are rendered");
        assert!(page.size.width > gpui::px(200.0));
        assert!(actions.origin.y >= page.origin.y);
        assert!(actions.origin.y + actions.size.height <= gpui::px(760.0));
        let card = cx.debug_bounds("connection-card").expect("the form sits on a card");
        assert!(page.origin.y >= card.origin.y && actions.origin.y <= card.origin.y + card.size.height);
        assert!(cx.debug_bounds("Prod").is_some(), "the environment chips are in the behaviour section");
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
        assert_eq!(view.connection_form.draft(cx).environment, bookie_core::Environment::Prod);
    });
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.connection_form.environment = bookie_core::Environment::Staging)
    });
    view.read_with(cx, |view, cx| {
        assert_eq!(view.connection_form.draft(cx).environment, bookie_core::Environment::Staging);
    });
}

#[gpui::test]
fn editing_a_coloured_connection_shows_its_colour_and_clearing_drops_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let id = uuid::Uuid::from_u128(3704);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.connections.push(bookie_app_services::read_session::ConnectionSummary {
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
            assert!(view.connection_form.status.contains("colour"));
            view.finish_connection_save(Ok((id, "orders".into(), vec![], true)), cx);
            assert!(!view.connection_form.status.contains("colour"));
            view.finish_connection_save(Err("Connection save failed".into()), cx);
            assert!(view.connection_form.status.contains("failed"));
            assert_eq!(view.connection_form.editing, Some(id));
        })
    });
}
