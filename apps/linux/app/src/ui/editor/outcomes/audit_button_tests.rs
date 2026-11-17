use super::*;

#[test]
fn several_audit_operations_get_numbered_buttons_and_a_single_one_stays_plain() {
    assert_eq!(audit_button_label(0, false), "View in Audit Log");
    assert_eq!(audit_button_label(0, true), "View in Audit Log (1)");
    assert_eq!(audit_button_label(1, true), "View in Audit Log (2)");
}
