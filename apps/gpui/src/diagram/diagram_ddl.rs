use bookie_app_services::cell_save_plan::CellSavePlan;
use bookie_core::TableInfo;
use bookie_core::sql_ddl::{DraftColumn, StructureOp, materialize_ops};

use crate::save_ui::ReviewLabels;
use crate::surfaces::{ButtonKind, compact_button, segment, segmented};
use crate::*;

const MAX_DRAFT_COLUMNS: usize = 32;

pub(crate) struct DiagramDdl {
    table: Option<TableInfo>,
    schema: Entity<TextInput>,
    name: Entity<TextInput>,
    columns: Vec<DraftFields>,
}

struct DraftFields {
    name: Entity<TextInput>,
    data_type: Entity<TextInput>,
    nullable: bool,
    primary_key: bool,
}

impl DraftFields {
    fn new(cx: &mut Context<GridView>) -> Self {
        Self {
            name: cx.new(|cx| TextInput::new("Column name", cx)),
            data_type: cx.new(|cx| TextInput::new("SQL data type", cx)),
            nullable: true,
            primary_key: false,
        }
    }

    fn column(&self, cx: &App) -> DraftColumn {
        let mut column = column(self.name.read(cx).content(), self.data_type.read(cx).content());
        column.primary_key = self.primary_key;
        column.nullable = self.nullable && !self.primary_key;
        column
    }
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
        self.is_writable()
            && self.live.as_ref().is_some_and(|live| {
                drivers_registry::default_registry()
                    .get(&live.driver_id)
                    .is_some_and(|driver| driver.capabilities().ddl_is_transactional)
            })
            && !self.transaction_open()
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
        let columns = vec![DraftFields::new(cx)];
        let field = if table.is_some() { &columns[0].name } else { &name };
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
        let columns = draft.columns.iter().map(|fields| fields.column(cx)).collect();
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
            destructive: false,
            note: Some(crate::structure_ui::STRUCTURE_CHANGE_NOTE),
            reconnect: create.then(|| table.clone()),
        };
        self.prepare_diagram_ddl(live, table, create, plan, labels, cx);
    }

    pub(crate) fn diagram_ddl_editor(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let draft = self.search.diagram.ddl.as_ref()?;
        Some(
            div()
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
                    draft.columns.iter().enumerate().map(|(index, fields)| {
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .py_1()
                            .child(div().flex_1().min_w_0().child(fields.name.clone()))
                            .child(div().flex_1().min_w_0().child(fields.data_type.clone()))
                            .children(
                                draft
                                    .table
                                    .is_none()
                                    .then(|| self.diagram_column_constraints(index, fields, theme, cx)),
                            )
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

    fn diagram_column_constraints(
        &self,
        index: usize,
        fields: &DraftFields,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let choices = [
            ("Nullable", true, false),
            ("Not null", false, false),
            ("Primary key", false, true),
        ];
        let current = (fields.nullable && !fields.primary_key, fields.primary_key);
        segmented(
            choices
                .into_iter()
                .enumerate()
                .map(|(choice, (label, nullable, primary_key))| {
                    segment(
                        ("diagram-draft-constraint", index * choices.len() + choice),
                        label,
                        current == (nullable, primary_key),
                        theme,
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if let Some(fields) = view
                            .search
                            .diagram
                            .ddl
                            .as_mut()
                            .and_then(|draft| draft.columns.get_mut(index))
                        {
                            fields.nullable = nullable;
                            fields.primary_key = primary_key;
                            cx.notify();
                        }
                    }))
                }),
            theme,
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
                            let fields = DraftFields::new(cx);
                            window.focus(&fields.name.read(cx).focus_handle(cx), cx);
                            draft.columns.push(fields);
                            cx.notify();
                        }
                    }),
                )
            }))
            .children(self.diagram_can_draft().then(|| {
                compact_button(ButtonKind::Primary, "diagram-ddl-review", "Review SQL", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.review_diagram_ddl(cx)))
            }))
            .child(
                compact_button(ButtonKind::Ghost, "diagram-ddl-cancel", "Cancel", theme).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.search.diagram.ddl = None;
                        cx.notify();
                    },
                )),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn diagram_creation_waits_for_review_and_refreshes_the_diagram(cx: &mut gpui::TestAppContext) {
        use crate::save_ui_tests::{saving_view, settle_saving, write_session};
        use crate::ui_tests::test_kit::{orders, until};
        use bookie_gpui::live::{LiveSession, Origin};
        let path = orders();
        let (view, cx) = saving_view(cx, &path);
        let write = write_session(&path, false);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.search.origin = Some(Origin::File(path.to_path_buf()));
                view.tabs.open_diagram();
                let id = view.live.as_ref().and_then(|live| live.writable).expect("writable");
                view.save.write = Some((id, write));
                view.begin_diagram_ddl(None, window, cx);
                let draft = view.search.diagram.ddl.as_ref().expect("draft");
                draft.name.update(cx, |input, cx| input.set_content("draft table", cx));
                draft.columns[0]
                    .name
                    .update(cx, |input, cx| input.set_content("entry", cx));
                draft.columns[0]
                    .data_type
                    .update(cx, |input, cx| input.set_content("text", cx));
                view.review_diagram_ddl(cx);
            })
        });
        settle_saving(cx);
        view.read_with(cx, |view, _| {
            let review = view.save.pending.as_ref().expect("SQL review");
            assert!(review.plan.preview.contains("CREATE TABLE"));
            assert!(review.note.is_some(), "the review explains the structure change");
        });
        let current = || LiveSession::open(&Origin::File(path.to_path_buf())).expect("open current database");
        assert!(!current().tables.iter().any(|table| table.name == "draft table"));

        cx.update(|_, cx| view.update(cx, |view, cx| view.confirm_save(cx)));
        let listed = |view: &GridView| {
            view.catalog.tables.iter().any(|table| table.name == "draft table")
                && view
                    .live
                    .as_ref()
                    .is_some_and(|live| live.tables.iter().any(|table| table.name == "draft table"))
        };
        until(&view, cx, "the new table is listed", |view| {
            !view.save.busy && !view.search.diagram.loading && listed(view)
        });
        view.read_with(cx, |view, _| {
            assert!(listed(view), "the sidebar lists the new table: {}", view.status);
            assert!(matches!(view.tabs.current(), Tab::Diagram));
            let diagram = view.search.diagram.view.as_ref().expect(&view.status);
            assert!(
                diagram.diagram.tables.iter().any(|table| table.name == "draft table"),
                "{}",
                view.status
            );
        });
        assert!(current().tables.iter().any(|table| table.name == "draft table"));
    }

    #[gpui::test]
    fn the_draft_form_puts_review_first_and_keeps_the_nullable_choice_in_place(cx: &mut gpui::TestAppContext) {
        use crate::save_ui_tests::{saving_view, settle_saving};
        use crate::ui_tests::test_kit::orders;
        let path = orders();
        let (view, cx) = saving_view(cx, &path);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.tabs.open_diagram();
                view.begin_diagram_ddl(None, window, cx);
            })
        });
        settle_saving(cx);
        let bounds = |cx: &mut gpui::VisualTestContext, id: &'static str| cx.debug_bounds(id).expect(id);
        let review = bounds(cx, "diagram-ddl-review");
        let cancel = bounds(cx, "diagram-ddl-cancel");
        assert!(
            review.origin.x < cancel.origin.x,
            "the primary action comes before Cancel"
        );
        let nullable = bounds(cx, "Nullable");
        let primary = bounds(cx, "Primary key");
        cx.simulate_click(primary.center(), Default::default());
        settle_saving(cx);
        view.read_with(cx, |view, cx| {
            let draft = view.search.diagram.ddl.as_ref().expect("draft");
            let column = draft.columns[0].column(cx);
            assert!(column.primary_key && !column.nullable);
        });
        assert_eq!(bounds(cx, "Nullable"), nullable, "the nullable choice keeps its place");
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
