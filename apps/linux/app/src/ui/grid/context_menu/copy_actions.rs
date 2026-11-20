use super::*;

pub(super) fn add_copy_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    add_text_copy_actions(group, setup);
    add_structured_copy_actions(group, setup);
    add_copy_in_clause_action(group, setup);
}

pub(super) fn add_text_copy_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let context = &setup.context;
    let sender = &setup.sender;
    let column_view = &setup.column_view;
    let columns = &setup.columns;
    let selection = &setup.column_selection;
    group.add_action_entries([
        copy_rows_action(
            "copy-rows",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            |columns, rows| Ok(export::render_tsv(columns, rows, false)),
        ),
        copy_rows_action(
            "copy-rows-headers",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            |columns, rows| Ok(export::render_tsv(columns, rows, true)),
        ),
        copy_rows_action(
            "copy-csv",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            |columns, rows| Ok(export::render_csv(columns, rows, false)),
        ),
        copy_rows_action(
            "copy-csv-headers",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            |columns, rows| Ok(export::render_csv(columns, rows, true)),
        ),
        copy_rows_action(
            "copy-text-table",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            |columns, rows| Ok(export::render_text_table(columns, rows)),
        ),
    ]);
}

pub(super) fn add_structured_copy_actions(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let context = &setup.context;
    let sender = &setup.sender;
    let column_view = &setup.column_view;
    let columns = &setup.columns;
    let selection = &setup.column_selection;
    let driver_id = setup.driver_id.clone();
    group.add_action_entries([
        copy_rows_action(
            "copy-json",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            |columns, rows| Ok(export::render_json(columns, rows)),
        ),
        copy_rows_action(
            "copy-markdown",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            |columns, rows| Ok(export::render_markdown(columns, rows)),
        ),
        copy_rows_action(
            "copy-json-lines",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            |columns, rows| Ok(export::render_json_lines(columns, rows)),
        ),
        copy_rows_action(
            "copy-values-list",
            context.clone(),
            sender.clone(),
            column_view,
            columns.clone(),
            selection.clone(),
            move |columns, rows| {
                export::render_value_list(&driver_id, columns, rows).map_err(|_| {
                    bookie_app_services::tr!(
                        "These values cannot be copied as SQL literals for this database. The clipboard was not changed."
                    )
                })
            },
        ),
    ]);
}

pub(super) fn add_copy_in_clause_action(group: &gio::SimpleActionGroup, setup: &MenuActionSetup) {
    let selection = setup.column_selection.clone();
    let context = setup.context.clone();
    let sender = setup.sender.clone();
    let view = setup.column_view.downgrade();
    let driver_id = setup.driver_id.clone();
    group.add_action_entries([gio::ActionEntry::builder("copy-in-clause")
        .activate(move |_, _, _| {
            let context = context.snapshot();
            let Some(slot) = context.as_ref() else { return };
            let Some(view) = view.upgrade() else { return };
            let column_rows = if selection.borrow().contains(slot.col_index) {
                crate::ui::grid::column_select::project_rows(
                    &[slot.col_index],
                    0..view.model().map_or(0, |model| model.n_items()),
                    |position| row_object_at(&view, position),
                )
            } else {
                values_for_column_menu(&view, position(slot), slot.col_index)
            };
            let Some(rows) = column_rows else {
                sender.send(GridMsg::IncompleteRowData).ok();
                return;
            };
            let clause = export::render_in_clause(&driver_id, &rows, 0);
            show_in_clause_skips(&view, clause.skipped);
            if !clause.sql.is_empty() {
                sender.send(GridMsg::CopyToClipboard(clause.sql)).ok();
            }
        })
        .build()]);
}

pub(super) fn show_in_clause_skips(view: &gtk::ColumnView, skipped: usize) {
    if skipped == 0 {
        return;
    }
    let message = bookie_app_services::tr!("Skipped {n} NULL, binary or unsupported values.")
        .replace("{n}", &skipped.to_string());
    let alert = libadwaita::AlertDialog::new(Some(&bookie_app_services::tr!("Copy as IN clause")), Some(&message));
    use libadwaita::prelude::*;
    alert.add_response("close", &bookie_app_services::tr!("Close"));
    alert.set_close_response("close");
    alert.present(Some(view));
}
