use std::sync::Arc;

use relm4::adw::prelude::*;
use relm4::{adw, gtk, gtk::glib};
use tokio_util::sync::CancellationToken;

use bookie_app_services::er_diagram::read::{SchemaRead, read_schema};
use bookie_app_services::schema_compare::{SchemaCompare, compare_schemas};
use bookie_core::{DriverCapabilities, DriverError, TableInfo};

use crate::services::database_service::DatabaseService;
use crate::services::preferences::PreferencesStore;
use crate::tr;

pub struct CompareRequest {
    pub schema: Option<String>,
    pub connection_id: Option<uuid::Uuid>,
    pub driver_id: String,
    pub capabilities: DriverCapabilities,
    pub scope: CancellationToken,
    pub hidden_schemas: Vec<String>,
}

pub fn present(
    parent: &gtk::Window,
    request: CompareRequest,
    database: &Arc<DatabaseService>,
    preferences: &PreferencesStore,
    on_script: impl Fn(String) + 'static,
) {
    let Some(connection) = request.connection_id.and_then(|id| database.get(id)) else {
        return notify(
            parent,
            &tr!("No active connection"),
            &tr!("Open a connection before comparing schemas."),
        );
    };
    let timeout = crate::services::operation_control::timeout_for(preferences, database, request.connection_id);
    let parent = parent.clone();
    glib::spawn_future_local(async move {
        if request
            .schema
            .as_ref()
            .is_some_and(|schema| request.hidden_schemas.iter().any(|hidden| hidden == schema))
        {
            return notify(
                &parent,
                &tr!("Hidden schema"),
                &tr!("Show the schema before comparing it."),
            );
        }
        let control = crate::services::operation_control::bounded_with(timeout, request.scope.child_token());
        let tables = match crate::services::catalog::list_tables_excluding(
            connection.as_ref(),
            &request.hidden_schemas,
            &control,
        )
        .await
        {
            Ok(tables) => tables,
            Err(error) => return failed(&parent, &error),
        };
        let others = other_schemas(&tables, request.schema.as_deref());
        if others.is_empty() {
            return notify(
                &parent,
                &tr!("Nothing to compare"),
                &tr!("This connection has no other schema to compare with."),
            );
        }
        let Some(desired) = choose_schema(&parent, request.schema.as_deref(), &others).await else {
            return;
        };
        let compared = compare_chosen(&connection, &request, &control, &tables, &desired).await;
        match compared {
            Ok(result) => {
                let summary = summary(&result);
                on_script(result.script);
                notify(&parent, &tr!("Schema compare"), &summary);
            }
            Err(Failure::Driver(error)) => failed(&parent, &error),
            Err(Failure::Message(message)) => notify(&parent, &tr!("Can't compare these schemas"), &message),
        }
    });
}

enum Failure {
    Driver(DriverError),
    Message(String),
}

async fn compare_chosen(
    connection: &Arc<dyn bookie_core::Connection>,
    request: &CompareRequest,
    control: &bookie_core::OperationControl,
    tables: &[TableInfo],
    desired: &str,
) -> Result<SchemaCompare, Failure> {
    let read = |schema: Option<String>| {
        let listed = tables_in(tables, schema.as_deref());
        async move {
            read_schema(
                connection.as_ref(),
                schema.as_deref(),
                &listed,
                request.capabilities,
                control,
                |_, _| {},
            )
            .await
            .map_err(Failure::Driver)
        }
    };
    let current = read(request.schema.clone()).await?;
    let wanted = read(Some(desired.to_string())).await?;
    both_complete(&current, &wanted).map_err(Failure::Message)?;
    compare_schemas(
        &request.driver_id,
        request.schema.as_deref(),
        &current.reads,
        Some(desired),
        &wanted.reads,
    )
    .map_err(|error| Failure::Message(error.to_string()))
}

async fn choose_schema(parent: &gtk::Window, current: Option<&str>, others: &[String]) -> Option<String> {
    let names: Vec<&str> = others.iter().map(String::as_str).collect();
    let dropdown = gtk::DropDown::from_strings(&names);
    let dialog = adw::AlertDialog::new(
        Some(&tr!("Compare schema")),
        Some(
            &tr!("Write a script that changes “{schema}” to match the schema you choose. Nothing runs until you run it from the editor.")
                .replace("{schema}", current.unwrap_or_default()),
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

fn notify(parent: &gtk::Window, title: &str, body: &str) {
    let alert = adw::AlertDialog::new(Some(title), Some(body));
    alert.add_response("ok", &tr!("OK"));
    alert.present(Some(parent));
}

fn failed(parent: &gtk::Window, error: &DriverError) {
    if matches!(error, DriverError::Cancelled) {
        return;
    }
    notify(
        parent,
        &tr!("Can't compare these schemas"),
        &crate::services::error_text::driver_message(error),
    );
}

pub(crate) fn other_schemas(tables: &[TableInfo], current: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = tables
        .iter()
        .filter_map(|table| table.schema.clone())
        .filter(|schema| Some(schema.as_str()) != current)
        .collect();
    names.sort();
    names.dedup();
    names
}

pub(crate) fn tables_in(tables: &[TableInfo], schema: Option<&str>) -> Vec<TableInfo> {
    tables
        .iter()
        .filter(|table| table.schema.as_deref() == schema)
        .cloned()
        .collect()
}

pub(crate) fn both_complete(current: &SchemaRead, desired: &SchemaRead) -> Result<(), String> {
    if current.truncated() || desired.truncated() {
        return Err(tr!(
            "One of the schemas has more tables than BookiE reads at once, so a comparison would report tables as missing that are not."
        ));
    }
    let unread: Vec<&str> = current
        .skipped
        .iter()
        .chain(&desired.skipped)
        .map(String::as_str)
        .collect();
    if unread.is_empty() {
        return Ok(());
    }
    Err(
        tr!("These tables could not be read, so the comparison would be wrong: {tables}")
            .replace("{tables}", &unread.join(", ")),
    )
}

pub(crate) fn summary(result: &SchemaCompare) -> String {
    if result.is_identical() {
        return tr!("The schemas match. The editor script has nothing to change.");
    }
    tr!("{added} new, {altered} changed and {dropped} dropped tables. The script is open in a new editor tab for review.")
        .replace("{added}", &result.added.len().to_string())
        .replace("{altered}", &result.altered.len().to_string())
        .replace("{dropped}", &result.dropped.len().to_string())
}

#[cfg(test)]
#[path = "schema_compare_tests.rs"]
mod tests;
