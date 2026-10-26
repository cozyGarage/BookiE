use super::*;

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_failure_without_a_receipt_does_not_offer_audit_navigation() {
    use relm4::gtk::prelude::*;
    relm4::adw::init().unwrap();
    let page = super::super::error_page(0, &"syntax error at position 8".into());
    let mut widgets = vec![page.clone().upcast::<relm4::gtk::Widget>()];
    let mut copy = false;
    let mut audit = false;
    let mut description = false;
    while let Some(widget) = widgets.pop() {
        if let Some(button) = widget.downcast_ref::<relm4::gtk::Button>() {
            copy |= button.label().as_deref() == Some("Copy error");
            audit |= button.action_name().as_deref() == Some("win.show-audit")
                || button.action_name().as_deref() == Some("win.show-audit-for");
        }
        if let Some(label) = widget.downcast_ref::<relm4::gtk::Label>() {
            description |= label.text() == "syntax error at position 8";
        }
        let mut child = widget.first_child();
        while let Some(next) = child {
            child = next.next_sibling();
            widgets.push(next);
        }
    }
    assert!(copy);
    assert!(!audit);
    assert!(description);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_failure_action_targets_its_exact_audit_operation() {
    use relm4::gtk::prelude::*;
    relm4::adw::init().unwrap();
    let operation_id = uuid::Uuid::new_v4();
    let failure = super::super::super::StatementFailure::from("query failed")
        .with_audit_receipts([tablepro_policy::AuditReceipt { operation_id }]);
    let page = super::super::error_page(0, &failure);
    let mut widgets = vec![page.clone().upcast::<relm4::gtk::Widget>()];
    let mut action_target = None;
    let mut visible_id = false;
    while let Some(widget) = widgets.pop() {
        if let Some(button) = widget.downcast_ref::<relm4::gtk::Button>()
            && button.action_name().as_deref() == Some("win.show-audit-for")
        {
            action_target = button
                .action_target_value()
                .and_then(|target| target.str().map(str::to_string));
        }
        if let Some(label) = widget.downcast_ref::<relm4::gtk::Label>() {
            visible_id |= label.text().contains(&operation_id.to_string());
        }
        let mut child = widget.first_child();
        while let Some(next) = child {
            child = next.next_sibling();
            widgets.push(next);
        }
    }
    assert_eq!(action_target.as_deref(), Some(operation_id.to_string().as_str()));
    assert!(visible_id);
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_refused_statement_keeps_its_recorded_audit_action() {
    relm4::adw::init().unwrap();
    let operation_id = uuid::Uuid::new_v4();
    let error = tablepro_core::DriverError::PolicyDenied("write refused".into());
    let failure = super::super::super::StatementFailure::located(&error, "DELETE FROM people", "DELETE FROM people")
        .with_audit_receipts([tablepro_policy::AuditReceipt { operation_id }]);
    let page = super::super::error_page(0, &failure);
    let widget = page.upcast::<gtk::Widget>();
    assert_eq!(
        audit_action_target(&widget).as_deref(),
        Some(operation_id.to_string().as_str())
    );
}

fn audit_action_target(widget: &gtk::Widget) -> Option<String> {
    if let Some(button) = widget.downcast_ref::<gtk::Button>()
        && button.action_name().as_deref() == Some("win.show-audit-for")
    {
        return button
            .action_target_value()
            .and_then(|target| target.str().map(str::to_string));
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(target) = audit_action_target(&current) {
            return Some(target);
        }
        child = current.next_sibling();
    }
    None
}

pub(super) fn contains_label(widget: &gtk::Widget, text: &str) -> bool {
    if let Some(label) = widget.downcast_ref::<gtk::Label>()
        && label.text() == text
    {
        return true;
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if contains_label(&widget, text) {
            return true;
        }
        child = widget.next_sibling();
    }
    false
}
