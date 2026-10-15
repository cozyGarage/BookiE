use relm4::{ComponentController, ComponentSender};

use super::{App, AppMsg};

impl App {
    pub(super) fn on_editor_needs_columns(&self, tables: Vec<String>, sender: ComponentSender<Self>) {
        let Some((conn, identity)) = self.connection_id.and_then(|id| self.database.get_with_identity(id)) else {
            return;
        };
        let pending: Vec<crate::ui::editor::SchemaRequest> = {
            let mut index = self.schema_index.borrow_mut();
            if index.sync_connection(&identity) {
                self.reset_schema_fetches();
            }
            let mut requested = self.requested_columns.borrow_mut();
            tables
                .into_iter()
                .filter(|table| !index.knows_columns(table))
                .filter(|table| requested.insert(crate::ui::editor::table_key(table)))
                .map(|table| index.request(table))
                .collect()
        };
        let grammar = self.schema_index.borrow().grammar;
        if pending.is_empty() {
            return;
        };
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let sender_clone = sender.clone();
        let token = self.schema_fetch_cancel.borrow().clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    for request in pending {
                        if token.is_cancelled() {
                            break;
                        }
                        let result = if let Some((schema, table)) = split_table_reference(&request.table, grammar) {
                            let control = crate::services::operation_control::bounded_with(timeout_secs, token.clone());
                            conn.fetch_columns_controlled(schema.as_deref(), &table, &control)
                                .await
                                .map(|columns| columns.into_iter().map(|column| column.name).collect())
                                .map_err(|_| ())
                        } else {
                            Err(())
                        };
                        sender_clone.input(AppMsg::SchemaColumnsFetched(request, result));
                    }
                })
                .drop_on_shutdown()
        });
    }

    fn reset_schema_fetches(&self) {
        self.requested_columns.borrow_mut().clear();
        crate::services::request_generation::replace_in_flight(&self.schema_fetch_cancel);
    }

    pub(super) fn on_schema_columns_fetched(
        &self,
        request: crate::ui::editor::SchemaRequest,
        columns: Result<Vec<String>, ()>,
    ) {
        let Some(identity) = self.connection_id.and_then(|id| self.database.identity(id)) else {
            return;
        };
        let key = crate::ui::editor::table_key(&request.table);
        let mut index = self.schema_index.borrow_mut();
        if index.sync_connection(&identity) {
            self.reset_schema_fetches();
        }
        if !index.accepts(&request) {
            return;
        }
        match columns {
            Ok(columns) => index.set_columns(&request.table, columns),
            Err(()) => {
                self.requested_columns.borrow_mut().remove(&key);
            }
        }
    }

    pub(super) fn rebuild_schema_buffer(&self) {
        if let Some(identity) = self.connection_id.and_then(|id| self.database.identity(id))
            && self.schema_index.borrow_mut().sync_connection(&identity)
        {
            self.reset_schema_fetches();
        }
        self.schema_index.borrow_mut().grammar = self
            .current_driver_id
            .as_deref()
            .and_then(crate::ui::editor::grammar_for);
        let mut words: Vec<String> = self.table_names.clone();
        let tabs = self.workspace_tabs.borrow();
        for tab in tabs.values() {
            if let Some(controller) = tab.browse_controller() {
                for col in controller.model().columns() {
                    words.push(col.name.clone());
                }
            }
        }
        words.sort_unstable();
        words.dedup();
        crate::ui::editor::update_schema_buffer(&self.schema_buffer, &words);

        let mut index = self.schema_index.borrow_mut();
        index.tables = self.table_names.clone();
        for tab in tabs.values() {
            if let Some(controller) = tab.browse_controller() {
                let model = controller.model();
                let columns: Vec<String> = model.columns().iter().map(|column| column.name.clone()).collect();
                if columns.is_empty() {
                    continue;
                }
                let reference = completion_relation_reference(
                    self.current_driver_id.as_deref().unwrap_or("postgres"),
                    model.schema(),
                    model.table(),
                );
                index.set_columns(&reference, columns);
            }
        }
    }
}

fn split_table_reference(
    reference: &str,
    grammar: Option<tablepro_core::sql_syntax::SqlGrammar>,
) -> Option<(Option<String>, String)> {
    let mut parts = crate::ui::editor::parse_reference_parts(reference, grammar)?
        .into_iter()
        .map(|part| part.text)
        .collect::<Vec<_>>();
    if parts.iter().any(String::is_empty) || parts.len() > 2 {
        return None;
    }
    let table = parts.pop()?;
    Some((parts.pop(), table))
}

pub(super) fn completion_table_reference(driver_id: &str, table: &tablepro_core::TableInfo) -> String {
    completion_relation_reference(driver_id, table.schema.as_deref(), &table.name)
}

pub(super) fn completion_catalog_names(
    driver_id: &str,
    tables: &[tablepro_core::TableInfo],
    views: &[tablepro_core::TableInfo],
) -> Vec<String> {
    tables
        .iter()
        .chain(views)
        .map(|table| completion_table_reference(driver_id, table))
        .collect()
}

fn completion_relation_reference(driver_id: &str, schema: Option<&str>, name: &str) -> String {
    let name = tablepro_core::sql_dialect::quote_ident(driver_id, name);
    schema.map_or(name.clone(), |schema| {
        format!("{}.{name}", tablepro_core::sql_dialect::quote_ident(driver_id, schema))
    })
}

#[cfg(test)]
mod tests {
    use super::{completion_table_reference, split_table_reference};
    use tablepro_core::TableInfo;
    use tablepro_core::sql_syntax::SqlGrammar;

    #[test]
    fn a_qualified_reference_splits_into_schema_and_table() {
        assert_eq!(
            split_table_reference("public.users", Some(SqlGrammar::PostgreSql)),
            Some((Some("public".to_string()), "users".to_string()))
        );
        assert_eq!(
            split_table_reference("\"public\".\"Users\"", Some(SqlGrammar::PostgreSql)),
            Some((Some("public".to_string()), "Users".to_string()))
        );
    }

    #[test]
    fn a_bare_reference_has_no_schema() {
        assert_eq!(
            split_table_reference("users", Some(SqlGrammar::PostgreSql)),
            Some((None, "users".to_string()))
        );
        assert_eq!(
            split_table_reference("`orders`", Some(SqlGrammar::MySql)),
            Some((None, "orders".to_string()))
        );
    }

    #[test]
    fn dots_and_doubled_quotes_inside_quotes_stay_in_the_name() {
        assert_eq!(
            split_table_reference("\"odd.schema\".\"a.b\"", Some(SqlGrammar::PostgreSql)),
            Some((Some("odd.schema".to_string()), "a.b".to_string()))
        );
        assert_eq!(
            split_table_reference("`odd.schema`.`a``b`", Some(SqlGrammar::MySql)),
            Some((Some("odd.schema".to_string()), "a`b".to_string()))
        );
        assert_eq!(
            split_table_reference("[dbo].[Items]", Some(SqlGrammar::MsSql)),
            Some((Some("dbo".to_string()), "Items".to_string()))
        );
        assert_eq!(
            split_table_reference("public.\"a\"\"b\"", Some(SqlGrammar::PostgreSql)),
            Some((Some("public".to_string()), "a\"b".to_string()))
        );
    }

    #[test]
    fn a_trailing_dot_is_not_a_schema_split() {
        assert_eq!(split_table_reference("users.", Some(SqlGrammar::PostgreSql)), None);
    }

    #[test]
    fn malformed_or_overqualified_references_are_refused() {
        assert_eq!(
            split_table_reference("public..users", Some(SqlGrammar::PostgreSql)),
            None
        );
        assert_eq!(
            split_table_reference("db.public.users", Some(SqlGrammar::PostgreSql)),
            None
        );
        assert_eq!(
            split_table_reference("\"unfinished", Some(SqlGrammar::PostgreSql)),
            None
        );
    }

    #[test]
    fn completion_catalog_names_preserve_quoted_identity_and_schema() {
        assert_eq!(
            completion_table_reference(
                "postgres",
                &TableInfo {
                    schema: Some("Public.Dot".into()),
                    name: "With Space".into(),
                }
            ),
            "\"Public.Dot\".\"With Space\""
        );
        let clickhouse_name = completion_table_reference(
            "clickhouse",
            &TableInfo {
                schema: Some("Odd\\Schema".into()),
                name: "With`Tick".into(),
            },
        );
        assert_eq!(clickhouse_name, "`Odd\\\\Schema`.`With``Tick`");
        assert_eq!(
            split_table_reference(&clickhouse_name, Some(SqlGrammar::ClickHouse)),
            Some((Some("Odd\\Schema".into()), "With`Tick".into()))
        );
    }
}
