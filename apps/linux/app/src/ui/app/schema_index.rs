use relm4::ComponentController;

use super::App;
use crate::ui::editor::catalog_reference;

impl App {
    pub(super) fn set_completion_tables(
        &mut self,
        tables: &[tablepro_core::TableInfo],
        views: &[tablepro_core::TableInfo],
    ) {
        let driver = self.current_driver_id.as_deref().unwrap_or("postgres");
        self.table_names = completion_names(driver, tables, views);
    }

    pub(super) fn refresh_schema_index(&self) {
        if let Some(identity) = self.connection_id.and_then(|id| self.database.identity(id)) {
            self.schema_index.borrow_mut().sync_connection(&identity);
        }
        self.schema_index.borrow_mut().grammar = self
            .current_driver_id
            .as_deref()
            .and_then(crate::ui::editor::grammar_for);
        let tabs = self.workspace_tabs.borrow();
        let mut index = self.schema_index.borrow_mut();
        index.set_tables(self.table_names.clone());
        for tab in tabs.values() {
            if let Some(editor) = tab.editor_controller() {
                let snapshot = index.editor_snapshot();
                let _ = editor
                    .sender()
                    .send(crate::ui::editor::SqlEditorInput::CompletionCatalogChanged(snapshot));
            }
            if let Some(controller) = tab.browse_controller() {
                let model = controller.model();
                let columns: Vec<String> = model.columns().iter().map(|column| column.name.clone()).collect();
                if columns.is_empty() {
                    continue;
                }
                let reference = catalog_reference(
                    self.current_driver_id.as_deref().unwrap_or("postgres"),
                    model.schema(),
                    model.table(),
                );
                index.set_columns(&reference, columns);
            }
        }
    }
}

fn completion_names(
    driver: &str,
    tables: &[tablepro_core::TableInfo],
    views: &[tablepro_core::TableInfo],
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    tables
        .iter()
        .chain(views)
        .map(|table| catalog_reference(driver, table.schema.as_deref(), &table.name))
        .filter(|name| seen.insert(name.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use tablepro_app_services::completion_identifiers::split_table_reference;
    use tablepro_core::sql_syntax::SqlGrammar;

    #[test]
    fn overlapping_view_names_appear_once_in_completion() {
        let view = tablepro_core::TableInfo {
            schema: Some("db".into()),
            name: "recent".into(),
        };
        let result = super::completion_names("mysql", std::slice::from_ref(&view), &[view.clone()]);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn quoted_names_reach_metadata_without_losing_identity() {
        for (sql, schema, table) in [
            (r#""odd.schema"."a.b""#, Some("odd.schema"), "a.b"),
            (r#"public."a""b""#, Some("public"), "a\"b"),
            (r#""public"."Users""#, Some("public"), "Users"),
            (r#"public."select""#, Some("public"), "select"),
            (r#""東京"."with space""#, Some("東京"), "with space"),
            ("PUBLIC.Users", Some("public"), "users"),
            ("users", None, "users"),
        ] {
            assert_eq!(
                split_table_reference(sql, Some(SqlGrammar::PostgreSql)),
                Some((schema.map(str::to_string), table.into()))
            );
        }
    }

    #[test]
    fn invalid_references_do_not_route_metadata() {
        for sql in [
            "users.",
            ".users",
            "a.b.c",
            r#"public."open"#,
            "public..users",
            "'users'",
            r#"public."""#,
            "users; DROP TABLE users",
        ] {
            assert!(
                split_table_reference(sql, Some(SqlGrammar::PostgreSql)).is_none(),
                "{sql}"
            );
        }
    }

    #[test]
    fn other_engines_keep_their_identifier_delimiters() {
        assert_eq!(
            split_table_reference("`orders`", Some(SqlGrammar::MySql)),
            Some((None, "orders".into()))
        );
        assert_eq!(
            split_table_reference("[dbo].[a]]b]", Some(SqlGrammar::MsSql)),
            Some((Some("dbo".into()), "a]b".into()))
        );
    }
}
