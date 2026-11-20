use bookie_app_services::health_report::report_label;
use bookie_core::{HealthReport, hidden_schemas_param};

use super::*;

pub(crate) const HEALTH_ACTIONS: [(&str, HealthReport); 5] = [
    ("health-vacuum", HealthReport::TableVacuum),
    ("health-bloat", HealthReport::EstimatedBloat),
    ("health-unused-indexes", HealthReport::UnusedIndexes),
    ("health-duplicate-indexes", HealthReport::DuplicateIndexes),
    ("health-seq-scans", HealthReport::SeqScanHeavy),
];

pub(crate) fn health_report_for(id: &str) -> Option<HealthReport> {
    HEALTH_ACTIONS
        .iter()
        .find_map(|(action, report)| (*action == id).then_some(*report))
}

pub(crate) fn health_label(report: HealthReport) -> String {
    format!("Health: {}", report_label(report))
}

impl GridView {
    pub(crate) fn execute_tool_palette_action(&mut self, id: &'static str, cx: &mut Context<Self>) -> bool {
        match id {
            "search-values" => self.open_value_search_bar(cx),
            "export-schema-diagram" => self.open_schema_diagram_bar(cx),
            "run-at-cursor" => self.run_at_cursor(cx),
            "paste-cells" => self.paste_cells(cx),
            _ => match health_report_for(id) {
                Some(report) => self.show_health_report(report, cx),
                None => return false,
            },
        }
        true
    }

    pub(crate) fn health_actions(&self) -> Vec<Action> {
        if !self.health_supported() {
            return Vec::new();
        }
        HEALTH_ACTIONS
            .iter()
            .map(|(id, report)| Action {
                id,
                label: health_label(*report),
                enabled: true,
            })
            .collect()
    }

    pub(crate) fn health_supported(&self) -> bool {
        self.live.as_ref().is_some_and(|session| {
            drivers_registry::default_registry()
                .get(&session.driver_id)
                .is_some_and(|driver| driver.capabilities().supports_table_health_report)
        })
    }

    pub(crate) fn show_health_report(&mut self, report: HealthReport, cx: &mut Context<Self>) {
        let Some(session) = self.live.clone().filter(|_| self.health_supported()) else {
            self.status = "The table and index health reports are available for PostgreSQL connections".into();
            return;
        };
        self.tabs.open_query();
        self.sync_grid_owner();
        self.status = format!("Reading {}…", report_label(report).to_lowercase());
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| {
                let started = Instant::now();
                let params = [hidden_schemas_param(&[])];
                let result =
                    session.run_query_bound(&report.sql(), &params, bookie_core::HEALTH_ROW_LIMIT as u64, &cancel);
                (started.elapsed(), result)
            },
            move |this, (elapsed, result)| match result {
                Ok(table) => {
                    this.add_run(Some(std::sync::Arc::new(table)));
                    if let Some(slot) = this.run_tabs.slots.last_mut() {
                        slot.run.elapsed_ms = Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX));
                    }
                    this.status = bookie_app_services::health_report::report_hint(report);
                }
                Err(message) => this.status = message,
            },
        );
    }
}

#[cfg(test)]
#[path = "health_ui_tests.rs"]
mod tests;
