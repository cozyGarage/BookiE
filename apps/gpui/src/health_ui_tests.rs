use super::{HEALTH_ACTIONS, health_label, health_report_for};

#[test]
fn every_report_has_one_action_with_a_distinct_id_and_label() {
    let mut ids: Vec<&str> = HEALTH_ACTIONS.iter().map(|(id, _)| *id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), bookie_core::HealthReport::ALL.len());
    for report in bookie_core::HealthReport::ALL {
        assert!(HEALTH_ACTIONS.iter().any(|(_, listed)| *listed == report));
        assert!(health_label(report).starts_with("Health: "));
    }
}

#[test]
fn an_action_id_maps_back_to_its_report_and_other_ids_do_not() {
    assert_eq!(
        health_report_for("health-vacuum"),
        Some(bookie_core::HealthReport::TableVacuum)
    );
    assert_eq!(health_report_for("data-dictionary"), None);
}
