use bookie_app_services::ssh_prompt::PendingHostKey;
use gpui::TestAppContext;
use tokio::sync::oneshot;

use crate::ui_tests::open;

fn pending(host: &str) -> (PendingHostKey, oneshot::Receiver<bool>) {
    let (respond, answer) = oneshot::channel();
    let pending = PendingHostKey {
        host: host.into(),
        algorithm: "ssh-ed25519".into(),
        fingerprint: "SHA256:abc".into(),
        respond,
    };
    (pending, answer)
}

fn click(cx: &mut gpui::VisualTestContext, id: &'static str) {
    let bounds = cx.debug_bounds(id).unwrap_or_else(|| panic!("{id} is not drawn"));
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
}

#[gpui::test]
fn an_unknown_host_key_shows_its_fingerprint_and_trust_reaches_the_ssh_handshake(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let (question, mut answer) = pending("db.example:22");
    let tx = view.read_with(cx, |view, _| view.host_key.tx.clone());
    assert!(tx.send(question).is_ok());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let open = view.host_key.current.as_ref().expect("the prompt is shown");
        let detail = crate::host_key_ui::host_key_detail(open);
        assert!(detail.contains("db.example:22") && detail.contains("ssh-ed25519") && detail.contains("SHA256:abc"));
    });
    click(cx, "trust-host-key");
    assert_eq!(answer.try_recv().ok(), Some(true));
    view.read_with(cx, |view, _| assert!(view.host_key.current.is_none()));
}

#[gpui::test]
fn cancelling_the_host_key_prompt_refuses_the_key(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let (question, mut answer) = pending("db.example:22");
    let tx = view.read_with(cx, |view, _| view.host_key.tx.clone());
    assert!(tx.send(question).is_ok());
    cx.run_until_parked();
    click(cx, "refuse-host-key");
    assert_eq!(answer.try_recv().ok(), Some(false));
    view.read_with(cx, |view, _| assert!(view.host_key.current.is_none()));
}

#[gpui::test]
fn an_abandoned_connection_attempt_removes_its_prompt_and_shows_the_next(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let tx = view.read_with(cx, |view, _| view.host_key.tx.clone());
    let (first, first_answer) = pending("first:22");
    let (second, _second_answer) = pending("second:22");
    assert!(tx.send(first).is_ok() && tx.send(second).is_ok());
    cx.run_until_parked();
    drop(first_answer);
    cx.executor().advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let shown = view.host_key.current.as_ref().map(|open| open.host.clone());
        assert_eq!(shown.as_deref(), Some("second:22"));
    });
}
