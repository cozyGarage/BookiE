use bookie_app_services::cell_save_plan::CellSavePlan;
use bookie_core::TableInfo;
use bookie_core::sql_ddl::{DraftColumn, StructureOp, materialize_ops};

use super::*;
use crate::save_ui::ReviewLabels;
use crate::surfaces::{ButtonKind, compact_button};

const MAX_DRAFT_COLUMNS: usize = 32;

pub(crate) struct DiagramDdl {
    table: Option<TableInfo>,
    schema: Entity<TextInput>,
    name: Entity<TextInput>,
    columns: Vec<(Entity<TextInput>, Entity<TextInput>)>,
}

fn new_column(cx: &mut Context<GridView>) -> (Entity<TextInput>, Entity<TextInput>) {
    (
        cx.new(|cx| TextInput::new("Column name", cx)),
        cx.new(|cx| TextInput::new("SQL data type", cx)),
    )
}

fn column(name: &str, data_type: &str) -> DraftColumn {
    DraftColumn {
        original: None,
        name: name.trim().to_owned(),
        data_type: data_type.trim().to_owned(),
        nullable: true,
        primary_key: false,
        auto_increment: false,
        default_value: None,
        comment: None,
        collation: None,
    }
}

fn plan_ddl(driver: &str, table: &TableInfo, columns: Vec<DraftColumn>, create: bool) -> Result<CellSavePlan, String> {
    if columns.is_empty()
        || columns
            .iter()
            .any(|column| column.name.is_empty() || column.data_type.is_empty())
    {
        return Err("Enter a name and SQL type for every column".into());
    }
    let ops = if create {
        vec![StructureOp::CreateTable {
            schema: table.schema.clone(),
            table: table.name.clone(),
            columns,
            indexes: Vec::new(),
            fks: Vec::new(),
            default_collation: None,
        }]
    } else {
        columns
            .into_iter()
            .map(|column| StructureOp::AddColumn {
                schema: table.schema.clone(),
                table: table.name.clone(),
                column,
            })
            .collect()
    };
    let sql = materialize_ops(&ops, driver).map_err(|error| error.to_string())?;
    Ok(CellSavePlan {
        preview: sql.join(";\n"),
        statements: sql.into_iter().map(|statement| (statement, Vec::new())).collect(),
        expect_one: Vec::new(),
    })
}

impl GridView {
    pub(crate) fn diagram_can_draft(&self) -> bool {
        self.live.as_ref().is_some_and(|live| {
            live.writable.is_some()
                && matches!(
                    live.driver_id.as_str(),
                    "postgres" | "sqlite" | "mysql" | "mssql" | "duckdb"
                )
        }) && !self.transaction_open()
            && !self.save.busy
            && self.save.pending.is_none()
    }

    pub(crate) fn begin_diagram_ddl(&mut self, table: Option<TableInfo>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.diagram_can_draft() || self.form_open() {
            return;
        }
        let scope = self.search.diagram.scope.clone().unwrap_or_default();
        let schema = cx.new(|cx| {
            let mut input = TextInput::new("Schema", cx);
            input.set_content(&scope, cx);
            input
        });
        let name = cx.new(|cx| TextInput::new("Table name", cx));
        let columns = vec![new_column(cx)];
        let field = if table.is_some() { &columns[0].0 } else { &name };
        window.focus(&field.read(cx).focus_handle(cx), cx);
        self.search.diagram.relationship = None;
        self.search.diagram.ddl = Some(DiagramDdl {
            table,
            schema,
            name,
            columns,
        });
        cx.notify();
    }

    fn review_diagram_ddl(&mut self, cx: &mut Context<Self>) {
        let (Some(draft), Some(live)) = (self.search.diagram.ddl.as_ref(), self.live.clone()) else {
            return;
        };
        if !self.diagram_can_draft() {
            return;
        }
        let create = draft.table.is_none();
        let table = draft.table.clone().unwrap_or_else(|| {
            let schema = draft.schema.read(cx).content().trim();
            TableInfo {
                schema: (!schema.is_empty()).then(|| schema.to_owned()),
                name: draft.name.read(cx).content().trim().to_owned(),
            }
        });
        if table.name.is_empty() || (create && table.schema.is_none() && live.driver_id != "sqlite") {
            self.status = "Enter the table name and an explicit target schema".into();
            cx.notify();
            return;
        }
        let columns = draft
            .columns
            .iter()
            .map(|(name, data_type)| column(name.read(cx).content(), data_type.read(cx).content()))
            .collect();
        let plan = match plan_ddl(&live.driver_id, &table, columns, create) {
            Ok(plan) => plan,
            Err(message) => {
                self.status = message;
                cx.notify();
                return;
            }
        };
        let name = bookie_core::sql_dialect::qualified_table(&live.driver_id, table.schema.as_deref(), &table.name);
        let labels = ReviewLabels {
            title: if create {
                format!("Create table {name}?")
            } else {
                format!("Add columns to {name}?")
            },
            done: if create {
                format!("Created {name}")
            } else {
                format!("Added columns to {name}")
            },
            review: "Review the generated SQL before applying it".into(),
            confirm: "Apply",
            note: None,
            reconnect: None,
        };
        self.prepare_diagram_ddl(live, table, create, plan, labels, cx);
    }

    pub(crate) fn diagram_ddl_editor(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let draft = self.search.diagram.ddl.as_ref()?;
        Some(
            div()
                .id("diagram-ddl")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .gap_2()
                .p_2()
                .child(crate::surfaces::section_header(
                    if draft.table.is_some() {
                        "Add columns"
                    } else {
                        "Create table"
                    },
                    theme,
                ))
                .when(draft.table.is_none(), |form| {
                    form.child(draft.schema.clone()).child(draft.name.clone())
                })
                .children(draft.table.as_ref().map(|table| {
                    div().truncate().child(
                        table
                            .schema
                            .as_ref()
                            .map_or_else(|| table.name.clone(), |schema| format!("{schema}.{}", table.name)),
                    )
                }))
                .child(div().id("diagram-ddl-columns").overflow_y_scroll().min_h_0().children(
                    draft.columns.iter().enumerate().map(|(index, (name, data_type))| {
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .py_1()
                            .child(div().flex_1().min_w_0().child(name.clone()))
                            .child(div().flex_1().min_w_0().child(data_type.clone()))
                            .children((draft.columns.len() > 1).then(|| {
                                compact_button(ButtonKind::Ghost, ("remove-draft-column", index), "Remove", theme)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(draft) = &mut this.search.diagram.ddl
                                            && index < draft.columns.len()
                                            && draft.columns.len() > 1
                                        {
                                            draft.columns.remove(index);
                                            cx.notify();
                                        }
                                    }))
                            }))
                    }),
                ))
                .child(self.diagram_ddl_controls(draft, theme, cx)),
        )
    }

    fn diagram_ddl_controls(&self, draft: &DiagramDdl, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_wrap()
            .gap_1()
            .children((draft.columns.len() < MAX_DRAFT_COLUMNS).then(|| {
                compact_button(ButtonKind::Ghost, "diagram-draft-add-column", "Add column", theme).on_click(
                    cx.listener(|this, _, window, cx| {
                        if let Some(draft) = &mut this.search.diagram.ddl
                            && draft.columns.len() < MAX_DRAFT_COLUMNS
                        {
                            let fields = new_column(cx);
                            window.focus(&fields.0.read(cx).focus_handle(cx), cx);
                            draft.columns.push(fields);
                            cx.notify();
                        }
                    }),
                )
            }))
            .child(
                compact_button(ButtonKind::Ghost, "diagram-ddl-cancel", "Cancel", theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.search.diagram.ddl = None;
                        cx.notify();
                    },
                )),
            )
            .children(self.diagram_can_draft().then(|| {
                compact_button(ButtonKind::Primary, "diagram-ddl-review", "Review SQL", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.review_diagram_ddl(cx)))
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn diagram_creation_waits_for_review_and_refreshes_the_diagram(cx: &mut gpui::TestAppContext) {
        use crate::save_ui_tests::{fixture_copy, saving_view, settle_saving, write_session};
        use bookie_gpui::live::{LiveSession, Origin};
        let path = fixture_copy("diagram-create-review");
        let (view, cx) = saving_view(cx, &path);
        let write = write_session(&path, false);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.tabs.open_diagram();
                let id = view.live.as_ref().and_then(|live| live.writable).expect("writable");
                view.save.write = Some((id, write));
                view.begin_diagram_ddl(None, window, cx);
                let draft = view.search.diagram.ddl.as_ref().expect("draft");
                draft.name.update(cx, |input, cx| input.set_content("draft table", cx));
                draft.columns[0]
                    .0
                    .update(cx, |input, cx| input.set_content("entry", cx));
                draft.columns[0].1.update(cx, |input, cx| input.set_content("text", cx));
                view.review_diagram_ddl(cx);
            })
        });
        settle_saving(cx);
        view.read_with(cx, |view, _| {
            let review = view.save.pending.as_ref().expect("SQL review");
            assert!(review.plan.preview.contains("CREATE TABLE"));
        });
        let current = || LiveSession::open(&Origin::File(path.clone())).expect("open current database");
        assert!(!current().tables.iter().any(|table| table.name == "draft table"));

        cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
        for _ in 0..100 {
            settle_saving(cx);
            if view.read_with(cx, |view, _| !view.save.busy && !view.search.diagram.loading) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        view.read_with(cx, |view, _| {
            assert!(matches!(view.tabs.current(), Tab::Diagram));
            let diagram = view.search.diagram.view.as_ref().expect(&view.status);
            assert!(
                diagram.diagram.tables.iter().any(|table| table.name == "draft table"),
                "{}",
                view.status
            );
        });
        assert!(current().tables.iter().any(|table| table.name == "draft table"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn diagram_ddl_quotes_exact_targets_and_rejects_sql_in_types() {
        let table = TableInfo {
            schema: Some("tenant.one".into()),
            name: "Order Items".into(),
        };
        let plan = plan_ddl("postgres", &table, vec![column("Line Number", "integer")], true).unwrap();
        assert!(plan.preview.contains("\"tenant.one\".\"Order Items\""));
        assert!(plan.preview.contains("\"Line Number\""));
        assert!(
            plan_ddl(
                "postgres",
                &table,
                vec![column("line", "int); DROP TABLE users; --")],
                false
            )
            .is_err()
        );
    }
}
