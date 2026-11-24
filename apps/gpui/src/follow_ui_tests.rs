use gpui::TestAppContext;

use crate::ui_tests::open;

#[gpui::test]
fn a_follow_collects_messages_ignores_a_stale_run_and_ends_with_its_reason(cx: &mut TestAppContext) {
    use crate::follow_ui::{FollowEvent, follow_state_for_test};
    use bookie_app_services::read_session_store::SubscriptionMessage;
    let (view, cx) = open(cx);
    let message = |payload: &str| SubscriptionMessage {
        channel: "jobs".into(),
        pattern: None,
        payload: payload.into(),
        payload_bytes: payload.len(),
    };
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.panel.follow = Some(follow_state_for_test("jobs", 2));
            view.apply_follow_event(FollowEvent::Message(2, message("one")), cx);
            view.apply_follow_event(FollowEvent::Message(1, message("stale")), cx);
            view.apply_follow_event(FollowEvent::Message(2, message("two")), cx);
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel.follow.as_ref().map(|state| state.received), Some(2));
        let lines: Vec<&str> = view.panel.messages.entries().iter().map(|entry| entry.text.as_str()).collect();
        assert!(lines.contains(&"jobs · one") && lines.contains(&"jobs · two"), "{lines:?}");
        assert!(!lines.iter().any(|line| line.contains("stale")), "{lines:?}");
        assert_eq!(view.status, "Following jobs · 2 messages");
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.apply_follow_event(FollowEvent::Ended(1, Ok(9)), cx)));
    view.read_with(cx, |view, _| assert!(view.panel.follow.is_some(), "an end from an older follow is ignored"));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_follow_event(FollowEvent::Ended(2, Err("connection closed unexpectedly".into())), cx)
        })
    });
    view.read_with(cx, |view, _| {
        assert!(view.panel.follow.is_none(), "an ended follow leaves no state behind");
        let last = view.panel.messages.entries().last().expect("a closing line");
        assert_eq!(last.kind, bookie_gpui::messages::MessageKind::Error);
        assert!(last.text.contains("ended: connection closed unexpectedly"), "{}", last.text);
    });
}
