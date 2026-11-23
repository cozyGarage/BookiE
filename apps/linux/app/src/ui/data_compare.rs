use std::sync::Arc;

use relm4::adw::prelude::*;
use relm4::{adw, gtk, gtk::glib};
use tokio_util::sync::CancellationToken;

use bookie_app_services::data_compare::{DataCompare, DataCompareError, TableSide, compare_tables};
use bookie_app_services::tr;
use bookie_core::{DriverError, TableInfo};

use crate::services::database_service::DatabaseService;
use crate::services::preferences::PreferencesStore;

pub struct DataCompareRequest {
    pub schema: Option<String>,
    pub table: String,
    pub connection_id: Option<uuid::Uuid>,
    pub driver_id: String,
    pub scope: CancellationToken,
    pub hidden_schemas: Vec<String>,
}

pub fn present(
    parent: &gtk::Window,
    request: DataCompareRequest,
    database: &Arc<DatabaseService>,
    preferences: &PreferencesStore,
    on_script: impl Fn(String) + 'static,
) {
    let Some(connection) = request.connection_id.and_then(|id| database.get(id)) else {
        return notify(
            parent,
            &tr!("No active connection"),
            &tr!("Open a connection before comparing data."),
        );
    };
    let timeout = crate::services::operation_control::timeout_for(preferences, database, request.connection_id);
    let parent = parent.clone();
    glib::spawn_future_local(async move {
        let control = crate::services::operation_control::bounded_with(timeout, request.scope.child_token());
        let tables = match crate::services::catalog::list_tables_excluding(
            connection.as_ref(),
            &request.hidden_schemas,
            &control,
        )
        .await
        {
            Ok(tables) => tables,
            Err(error) => return failed(&parent, &DataCompareError::Driver(error)),
        };
        let others = other_tables(&tables, request.schema.as_deref(), &request.table);
        if others.is_empty() {
            return notify(
                &parent,
                &tr!("Nothing to compare"),
                &tr!("This connection has no other table to compare with."),
            );
        }
        let Some(source) = choose_table(&parent, &request.table, &others).await else {
            return;
        };
        let compared = compare_tables(
            &request.driver_id,
            TableSide {
                connection: connection.as_ref(),
                schema: source.schema.as_deref(),
                table: &source.name,
            },
            TableSide {
                connection: connection.as_ref(),
                schema: request.schema.as_deref(),
                table: &request.table,
            },
            None,
            &control,
        )
        .await;
        let result = match compared {
            Ok(result) => result,
            Err(error) => return failed(&parent, &error),
        };
        let Some(include_deletes) = review(&parent, &result, &request.driver_id).await else {
            return;
        };
        match result.script(
            &request.driver_id,
            request.schema.as_deref(),
            &request.table,
            include_deletes,
        ) {
            Ok(script) => on_script(script),
            Err(error) => failed(&parent, &error),
        }
    });
}

pub(crate) fn other_tables(tables: &[TableInfo], schema: Option<&str>, table: &str) -> Vec<TableInfo> {
    let mut others: Vec<TableInfo> = tables
        .iter()
        .filter(|other| other.schema.as_deref() != schema || other.name != table)
        .cloned()
        .collect();
    others.sort_by(|a, b| (&a.schema, &a.name).cmp(&(&b.schema, &b.name)));
    others
}

pub(crate) fn table_label(table: &TableInfo) -> String {
    match &table.schema {
        Some(schema) => format!("{schema}.{}", table.name),
        None => table.name.clone(),
    }
}

async fn choose_table(parent: &gtk::Window, current: &str, others: &[TableInfo]) -> Option<TableInfo> {
    let labels: Vec<String> = others.iter().map(table_label).collect();
    let names: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dropdown = gtk::DropDown::from_strings(&names);
    dropdown.set_enable_search(true);
    let dialog = adw::AlertDialog::new(
        Some(&tr!("Compare data")),
        Some(
            &tr!("Compare the rows of “{table}” with the table you choose, then review a script that changes “{table}” to match. Nothing runs until you run it from the editor.")
                .replace("{table}", current),
        ),
    );
    dialog.set_extra_child(Some(&dropdown));
    dialog.add_response("cancel", &tr!("Cancel"));
    dialog.add_response("compare", &tr!("Compare"));
    dialog.set_response_appearance("compare", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("compare"));
    dialog.set_close_response("cancel");
    if dialog.choose_future(Some(parent)).await != "compare" {
        return None;
    }
    others.get(dropdown.selected() as usize).cloned()
}

async fn review(parent: &gtk::Window, result: &DataCompare, driver_id: &str) -> Option<bool> {
    let content = gtk::Box::new(gtk::Orientation::Vertical, crate::ui::spacing::SM);
    let sample = result.sample_lines(driver_id).join("\n");
    if !sample.is_empty() {
        let label = gtk::Label::builder()
            .label(&sample)
            .selectable(true)
            .xalign(0.0)
            .wrap(false)
            .build();
        label.add_css_class("monospace");
        let scroller = gtk::ScrolledWindow::builder()
            .child(&label)
            .min_content_height(160)
            .max_content_height(320)
            .propagate_natural_height(true)
            .build();
        content.append(&scroller);
    }
    let deletes = gtk::CheckButton::with_label(&tr!("Include deletes in the script"));
    deletes.set_active(false);
    deletes.set_visible(result.counts.delete > 0);
    content.append(&deletes);
    let dialog = adw::AlertDialog::new(Some(&tr!("Data compare")), Some(&summary(result)));
    dialog.set_extra_child(Some(&content));
    dialog.add_response("close", &tr!("Close"));
    if !result.is_identical() {
        dialog.add_response("script", &tr!("Open Script"));
        dialog.set_response_appearance("script", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("script"));
    }
    dialog.set_close_response("close");
    if dialog.choose_future(Some(parent)).await != "script" {
        return None;
    }
    Some(deletes.is_active())
}

pub(crate) fn summary(result: &DataCompare) -> String {
    let mut text = if result.is_identical() {
        tr!("The tables match: {same} rows are the same.").replace("{same}", &result.counts.same.to_string())
    } else {
        tr!("{same} same, {insert} to insert, {update} to update and {delete} to delete. Deletes stay out of the script unless you include them.")
            .replace("{same}", &result.counts.same.to_string())
            .replace("{insert}", &result.counts.insert.to_string())
            .replace("{update}", &result.counts.update.to_string())
            .replace("{delete}", &result.counts.delete.to_string())
    };
    if !result.layout.not_compared.is_empty() {
        text.push('\n');
        text.push_str(&tr!("Not compared: {columns}.").replace("{columns}", &result.layout.not_compared.join(", ")));
    }
    if result.unknown_cells > 0 {
        text.push('\n');
        text.push_str(
            &tr!("{count} values could not be decoded and were not compared.")
                .replace("{count}", &result.unknown_cells.to_string()),
        );
    }
    text
}

fn notify(parent: &gtk::Window, title: &str, body: &str) {
    let alert = adw::AlertDialog::new(Some(title), Some(body));
    alert.add_response("ok", &tr!("OK"));
    alert.present(Some(parent));
}

fn failed(parent: &gtk::Window, error: &DataCompareError) {
    let message = match error {
        DataCompareError::Driver(DriverError::Cancelled) => return,
        DataCompareError::Driver(error) => crate::services::error_text::driver_message(error),
        other => other.to_string(),
    };
    notify(parent, &tr!("Can't compare these tables"), &message);
}

#[cfg(test)]
#[path = "data_compare_tests.rs"]
mod tests;
