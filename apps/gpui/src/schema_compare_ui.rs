use bookie_app_services::er_diagram::TableRead;
use bookie_app_services::schema_compare::{SchemaCompare, compare_schemas};
use bookie_gpui::palette::Action;
use gpui::{Context, Window};

use crate::{GridView, Work};

impl GridView {
    pub(crate) fn schema_compare_actions(&self) -> Vec<Action> {
        let enabled = self.live.as_ref().is_some_and(|s| !s.tables.is_empty());
        vec![Action {
            id: "schema-compare",
            label: "Compare schemas and generate migration script…".to_string(),
            enabled,
        }]
    }

    pub(crate) fn open_schema_compare(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.clone() else {
            self.status = "Connect to a database to compare schemas".into();
            return;
        };
        let current_schema = self.catalog.grid_table.schema.clone();
        self.status = "Comparing schemas…".into();
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| {
                let mut all_tables = Vec::new();
                for table in &live.tables {
                    if cancel.is_cancelled() {
                        return Err("Cancelled".to_string());
                    }
                    let columns = live.table_columns(table)?;
                    let foreign_keys = if live.driver_id == "postgres"
                        || live.driver_id == "mysql"
                        || live.driver_id == "sqlite"
                    {
                        live.table_structure(table)
                            .map(|s| s.foreign_keys)
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    all_tables.push(TableRead {
                        schema: table.schema.clone(),
                        name: table.name.clone(),
                        columns,
                        foreign_keys,
                    });
                }
                let current_tables: Vec<_> = all_tables
                    .iter()
                    .filter(|t| t.schema == current_schema)
                    .cloned()
                    .collect();
                let other_schema = all_tables
                    .iter()
                    .find_map(|t| (t.schema != current_schema).then(|| t.schema.clone()))
                    .flatten();
                let target_tables: Vec<_> = all_tables
                    .iter()
                    .filter(|t| t.schema == other_schema)
                    .cloned()
                    .collect();
                let diff = compare_schemas(
                    &live.driver_id,
                    current_schema.as_deref(),
                    &current_tables,
                    other_schema.as_deref(),
                    &target_tables,
                )
                .map_err(|error| error.to_string())?;
                Ok::<SchemaCompare, String>(diff)
            },
            move |this, outcome| match outcome {
                Ok(diff) => {
                    let summary = format!(
                        "Schema compare: {} added, {} dropped, {} altered",
                        diff.added.len(),
                        diff.dropped.len(),
                        diff.altered.len()
                    );
                    this.status = summary.clone();
                    this.panel
                        .messages
                        .push(bookie_gpui::messages::MessageKind::Success, summary);
                    this.tabs.open_query();
                    this.sync_grid_owner();
                    this.search.sql_file.opened = Some(diff.script);
                    this.focus_editor = true;
                }
                Err(error) => {
                    this.status = error.clone();
                    this.panel
                        .messages
                        .push(bookie_gpui::messages::MessageKind::Error, error);
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_compare_diff_detects_changes() {
        use bookie_app_services::er_diagram::TableRead;
        use bookie_core::ColumnInfo;

        let col = |name: &str| ColumnInfo {
            name: name.into(),
            data_type: "integer".into(),
            nullable: true,
            primary_key: name == "id",
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        };
        let t1 = TableRead {
            schema: Some("s1".into()),
            name: "users".into(),
            columns: vec![col("id"), col("name")],
            foreign_keys: Vec::new(),
        };
        let t2 = TableRead {
            schema: Some("s2".into()),
            name: "users".into(),
            columns: vec![col("id"), col("name"), col("email")],
            foreign_keys: Vec::new(),
        };
        let diff = compare_schemas("postgres", Some("s1"), &[t1], Some("s2"), &[t2]).unwrap();
        assert_eq!(diff.altered.len(), 1);
        assert!(diff.script.contains("ALTER TABLE"));
    }
}
