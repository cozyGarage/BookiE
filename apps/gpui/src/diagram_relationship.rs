use bookie_app_services::er_diagram::DiagramTable;
use bookie_core::{ForeignKeyInfo, TableInfo};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::Sizable as _;

use super::*;
use crate::diagram_ui::DiagramView;
use crate::surfaces::{ButtonKind, compact_button};

pub(crate) struct RelationshipDraft {
    source: TableInfo,
    name: Entity<TextInput>,
    target: Option<usize>,
    source_column: Option<usize>,
    target_column: Option<usize>,
    pairs: Vec<(usize, usize)>,
}

#[derive(Clone, Copy)]
enum Choice {
    Target,
    SourceColumn,
    TargetColumn,
}

fn relationship_choice(
    id: &'static str,
    label: &'static str,
    selected: Option<usize>,
    options: Vec<String>,
    choice: Choice,
    cx: &mut Context<GridView>,
) -> impl IntoElement {
    let title = selected
        .and_then(|index| options.get(index).cloned())
        .unwrap_or_else(|| label.into());
    let owner = cx.entity().downgrade();
    Button::new(id)
        .ghost()
        .small()
        .compact()
        .max_w_full()
        .label(title)
        .accessibility_label(label)
        .dropdown_menu(move |menu, _, _| {
            options.iter().enumerate().fold(menu, |menu, (index, label)| {
                let owner = owner.clone();
                menu.item(
                    PopupMenuItem::new(label.clone())
                        .checked(selected == Some(index))
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |this, cx| this.choose_relationship_option(choice, index, cx));
                        }),
                )
            })
        })
}

fn target_id(table: &DiagramTable) -> TableInfo {
    TableInfo {
        schema: table.schema.clone(),
        name: table.name.clone(),
    }
}

impl GridView {
    pub(crate) fn begin_diagram_relationship(
        &mut self,
        source: TableInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.transaction_open() {
            self.status = "Finish the transaction before editing relationships".into();
            cx.notify();
            return;
        }
        let name = cx.new(|cx| TextInput::new("Foreign key name", cx));
        window.focus(&name.read(cx).focus_handle(cx), cx);
        self.search.diagram.relationship = Some(RelationshipDraft {
            source,
            name,
            target: None,
            source_column: None,
            target_column: None,
            pairs: Vec::new(),
        });
        cx.notify();
    }

    fn choose_relationship_option(&mut self, choice: Choice, index: usize, cx: &mut Context<Self>) {
        let Some(draft) = &mut self.search.diagram.relationship else {
            return;
        };
        match choice {
            Choice::Target => {
                if draft.target != Some(index) {
                    draft.pairs.clear();
                    draft.target_column = None;
                }
                draft.target = Some(index);
            }
            Choice::SourceColumn => draft.source_column = Some(index),
            Choice::TargetColumn => draft.target_column = Some(index),
        }
        cx.notify();
    }

    fn add_relationship_pair(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = &mut self.search.diagram.relationship else {
            return;
        };
        let (Some(source), Some(target)) = (draft.source_column, draft.target_column) else {
            return;
        };
        if draft
            .pairs
            .iter()
            .any(|(left, right)| *left == source || *right == target)
        {
            self.status = "A foreign key column can only be paired once".into();
        } else {
            draft.pairs.push((source, target));
            draft.source_column = None;
            draft.target_column = None;
        }
        cx.notify();
    }

    fn prepare_diagram_relationship(&mut self, cx: &mut Context<Self>) {
        let state = &self.search.diagram;
        let (Some(view), Some(draft)) = (&state.view, &state.relationship) else {
            return;
        };
        let Some(source) = view
            .diagram
            .position_of(draft.source.schema.as_deref(), &draft.source.name)
        else {
            return;
        };
        let Some(target) = draft.target.and_then(|index| view.diagram.tables.get(index)) else {
            return;
        };
        let name = draft.name.read(cx).content().trim();
        if name.is_empty() || draft.pairs.is_empty() {
            self.status = "Name the foreign key and add at least one column pair".into();
            cx.notify();
            return;
        }
        let mut columns = Vec::new();
        let mut ref_columns = Vec::new();
        for &(left, right) in &draft.pairs {
            let (Some(left), Some(right)) = (view.diagram.tables[source].columns.get(left), target.columns.get(right))
            else {
                return;
            };
            columns.push(left.name.clone());
            ref_columns.push(right.name.clone());
        }
        let key = ForeignKeyInfo {
            name: name.into(),
            columns,
            ref_schema: target.schema.clone(),
            ref_table: target.name.clone(),
            ref_columns,
            on_delete: None,
            on_update: None,
        };
        let source = draft.source.clone();
        self.review_diagram_foreign_key(source, key, cx);
    }

    pub(crate) fn diagram_relationship_editor(
        &self,
        view: &DiagramView,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let draft = self.search.diagram.relationship.as_ref()?;
        let source = &view.diagram.tables[view
            .diagram
            .position_of(draft.source.schema.as_deref(), &draft.source.name)?];
        let target = draft.target.and_then(|index| view.diagram.tables.get(index));
        let mut editor = div()
            .id("diagram-relationship-draft")
            .flex()
            .flex_col()
            .gap_2()
            .min_h_0()
            .overflow_y_scroll()
            .child(div().child(format!("Foreign key from {}", table_label(&draft.source))))
            .child(draft.name.clone())
            .child(relationship_choice(
                "relationship-target",
                "Referenced table",
                draft.target,
                view.diagram
                    .tables
                    .iter()
                    .map(|table| table_label(&target_id(table)))
                    .collect(),
                Choice::Target,
                cx,
            ));
        if let Some(target) = target {
            editor = editor.child(self.relationship_columns(draft, source, target, theme, cx));
        }
        Some(
            editor.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        compact_button(ButtonKind::Primary, "review-diagram-relationship", "Review SQL", theme)
                            .on_click(cx.listener(|this, _, _, cx| this.prepare_diagram_relationship(cx))),
                    )
                    .child(
                        compact_button(ButtonKind::Ghost, "cancel-diagram-relationship", "Cancel", theme).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.search.diagram.relationship = None;
                                window.focus(&this.focus, cx);
                                cx.notify();
                            }),
                        ),
                    ),
            ),
        )
    }

    fn relationship_columns(
        &self,
        draft: &RelationshipDraft,
        source: &DiagramTable,
        target: &DiagramTable,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .children(draft.pairs.iter().enumerate().filter_map(|(index, &(left, right))| {
                let (left, right) = (source.columns.get(left)?, target.columns.get(right)?);
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(format!("{} → {}", left.name, right.name)),
                        )
                        .child(
                            compact_button(ButtonKind::Ghost, ("remove-column-pair", index), "Remove", theme).on_click(
                                cx.listener(move |this, _, _, cx| {
                                    if let Some(draft) = &mut this.search.diagram.relationship
                                        && index < draft.pairs.len()
                                    {
                                        draft.pairs.remove(index);
                                        cx.notify();
                                    }
                                }),
                            ),
                        ),
                )
            }))
            .child(relationship_choice(
                "relationship-source-column",
                "Source column",
                draft.source_column,
                source.columns.iter().map(|column| column.name.clone()).collect(),
                Choice::SourceColumn,
                cx,
            ))
            .child(relationship_choice(
                "relationship-target-column",
                "Referenced column",
                draft.target_column,
                target.columns.iter().map(|column| column.name.clone()).collect(),
                Choice::TargetColumn,
                cx,
            ))
            .child(
                compact_button(ButtonKind::Ghost, "add-column-pair", "Add column pair", theme)
                    .on_click(cx.listener(|this, _, _, cx| this.add_relationship_pair(cx))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save_ui_tests::{fixture_copy, saving_view, settle_saving};

    #[gpui::test]
    fn relationship_review_keeps_the_diagram_and_does_not_replace_the_active_load(cx: &mut gpui::TestAppContext) {
        let path = fixture_copy("diagram-relationship-review");
        let (view, cx) = saving_view(cx, &path);
        let generation = cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.tabs.open_diagram();
                let generation = view.loads.generation;
                let source = TableInfo {
                    schema: None,
                    name: "customers".into(),
                };
                assert_ne!(source, view.catalog.grid_table);
                view.review_diagram_foreign_key(
                    source,
                    ForeignKeyInfo {
                        name: "test_relationship".into(),
                        columns: vec!["id".into()],
                        ref_schema: None,
                        ref_table: "orders".into(),
                        ref_columns: vec!["id".into()],
                        on_delete: None,
                        on_update: None,
                    },
                    cx,
                );
                generation
            })
        });
        settle_saving(cx);
        view.read_with(cx, |view, _| {
            assert_eq!(view.loads.generation, generation);
            assert!(matches!(view.tabs.current(), Tab::Diagram));
            assert!(view.save.pending.is_none());
            assert!(view.status.contains("cannot add foreign keys"), "{}", view.status);
        });
        std::fs::remove_file(path).ok();
    }
}
