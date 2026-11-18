use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use relm4::adw::prelude::*;
use relm4::{adw, gtk, gtk::glib};
use tokio_util::sync::CancellationToken;

use bookie_app_services::er_diagram::read::{SchemaRead, read_schema};
use bookie_app_services::er_diagram::{SchemaDiagram, export, zoom};
use bookie_core::{DriverCapabilities, DriverError};

use super::canvas::DiagramCanvas;
use super::describe::{diagram_label, table_description};
use crate::services::database_service::DatabaseService;
use crate::services::preferences::PreferencesStore;
use bookie_app_services::tr;

struct Ui {
    dialog: adw::Dialog,
    stack: gtk::Stack,
    progress: gtk::Label,
    failure: adw::StatusPage,
    banner: adw::Banner,
    list: gtk::ListBox,
    canvas: DiagramCanvas,
    zoom_label: gtk::Label,
    export_button: gtk::MenuButton,
    exported: Rc<RefCell<SchemaDiagram>>,
}

#[derive(Clone)]
struct DictionaryExportContext {
    parent: glib::WeakRef<adw::ApplicationWindow>,
    connection: Arc<dyn bookie_core::Connection>,
    capabilities: DriverCapabilities,
    schema: Option<String>,
    hidden_schemas: Vec<String>,
    driver_id: String,
    token: CancellationToken,
    timeout: u32,
    preferences: PreferencesStore,
}

pub struct DiagramRequest {
    pub schema: Option<String>,
    pub connection_id: Option<uuid::Uuid>,
    pub driver_id: String,
    pub capabilities: DriverCapabilities,
    pub scope: CancellationToken,
    pub hidden_schemas: Vec<String>,
}

pub fn present(
    parent: &gtk::Window,
    request: DiagramRequest,
    database: &Arc<DatabaseService>,
    preferences: &PreferencesStore,
) {
    let Some(connection) = request.connection_id.and_then(|id| database.get(id)) else {
        show_no_connection(parent);
        return;
    };
    let token = request.scope.child_token();
    let timeout = crate::services::operation_control::timeout_for(preferences, database, request.connection_id);
    let context = dictionary_export_context(
        parent,
        &request,
        connection.clone(),
        token.clone(),
        timeout,
        preferences,
    );
    let ui = Rc::new(build(&diagram_label(request.schema.as_deref()), &token, context));
    ui.dialog.present(Some(parent));
    glib::spawn_future_local(async move {
        let control = crate::services::operation_control::bounded_with(timeout, token.clone());
        let listed = match request.schema.as_deref() {
            Some(schema) if request.hidden_schemas.iter().any(|hidden| hidden == schema) => Ok(Vec::new()),
            Some(schema) => connection.list_tables_in(&[schema.to_string()], &control).await,
            None => {
                crate::services::catalog::list_tables_excluding(connection.as_ref(), &request.hidden_schemas, &control)
                    .await
            }
        };
        let tables = match listed {
            Ok(tables) => tables
                .into_iter()
                .filter(|table| request.schema.is_none() || table.schema == request.schema)
                .collect::<Vec<_>>(),
            Err(error) => return show_failure(&ui, &token, &error),
        };
        let progress = {
            let ui = ui.clone();
            move |done: usize, total: usize| {
                ui.progress.set_text(
                    &tr!("Reading table {done} of {total}…")
                        .replace("{done}", &done.to_string())
                        .replace("{total}", &total.to_string()),
                );
            }
        };
        let outcome = read_schema(
            connection.as_ref(),
            request.schema.as_deref(),
            &tables,
            request.capabilities,
            &control,
            progress,
        )
        .await;
        match outcome {
            Ok(read) => show_diagram(&ui, read),
            Err(error) => show_failure(&ui, &token, &error),
        }
    });
}

fn show_no_connection(parent: &gtk::Window) {
    let alert = adw::AlertDialog::new(
        Some(&tr!("No active connection")),
        Some(&tr!("Open a connection before showing its diagram.")),
    );
    alert.add_response("ok", &tr!("OK"));
    alert.present(Some(parent));
}

fn dictionary_export_context(
    parent: &gtk::Window,
    request: &DiagramRequest,
    connection: Arc<dyn bookie_core::Connection>,
    token: CancellationToken,
    timeout: u32,
    preferences: &PreferencesStore,
) -> Option<DictionaryExportContext> {
    Some(DictionaryExportContext {
        parent: parent.clone().downcast::<adw::ApplicationWindow>().ok()?.downgrade(),
        connection,
        capabilities: request.capabilities,
        schema: request.schema.clone(),
        hidden_schemas: request.hidden_schemas.clone(),
        driver_id: request.driver_id.clone(),
        token,
        timeout,
        preferences: preferences.clone(),
    })
}

fn build(title: &str, token: &CancellationToken, context: Option<DictionaryExportContext>) -> Ui {
    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(1100)
        .content_height(720)
        .build();
    let canvas = DiagramCanvas::new();
    let zoom_label = gtk::Label::new(None);
    zoom_label.add_css_class("dim-label");
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .build();
    list.add_css_class("navigation-sidebar");
    let banner = adw::Banner::builder().revealed(false).use_markup(false).build();
    let stack = gtk::Stack::new();
    let progress = gtk::Label::new(Some(&tr!("Reading tables…")));
    let failure = adw::StatusPage::builder().icon_name("dialog-warning-symbolic").build();
    stack.add_named(&loading_page(&progress, &dialog, token), Some("loading"));
    stack.add_named(&failure, Some("failure"));
    stack.add_named(&ready_page(&banner, &list, &canvas), Some("ready"));
    canvas.connect_zoom_changed({
        let zoom_label = zoom_label.downgrade();
        move |level| {
            if let Some(zoom_label) = zoom_label.upgrade() {
                zoom_label.set_text(&format!("{}%", zoom::percent(level)));
            }
        }
    });
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&stack));
    let exported = Rc::new(RefCell::new(SchemaDiagram::default()));
    let export_button = export_button(&exported, &toasts, context);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header(&canvas, &zoom_label, &export_button));
    toolbar.set_content(Some(&toasts));
    dialog.set_child(Some(&toolbar));
    dialog.connect_closed({
        let token = token.clone();
        move |_| token.cancel()
    });
    Ui {
        dialog,
        stack,
        progress,
        failure,
        banner,
        list,
        canvas,
        zoom_label,
        export_button,
        exported,
    }
}

fn loading_page(progress: &gtk::Label, dialog: &adw::Dialog, token: &CancellationToken) -> gtk::Box {
    let cancel = gtk::Button::with_label(&tr!("Cancel"));
    cancel.set_halign(gtk::Align::Center);
    cancel.connect_clicked({
        let (dialog, token) = (dialog.clone(), token.clone());
        move |_| {
            token.cancel();
            dialog.close();
        }
    });
    let spinner = adw::Spinner::new();
    spinner.set_size_request(32, 32);
    let page = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .build();
    page.append(&spinner);
    page.append(progress);
    page.append(&cancel);
    page
}

fn ready_page(banner: &adw::Banner, list: &gtk::ListBox, canvas: &DiagramCanvas) -> gtk::Box {
    let sidebar = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_width(200)
        .child(list)
        .build();
    let split = gtk::Paned::builder()
        .start_child(&sidebar)
        .end_child(canvas)
        .position(220)
        .shrink_start_child(false)
        .resize_start_child(false)
        .vexpand(true)
        .build();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.append(banner);
    page.append(&split);
    page
}

fn header(canvas: &DiagramCanvas, zoom_label: &gtk::Label, export_button: &gtk::MenuButton) -> adw::HeaderBar {
    let bar = adw::HeaderBar::new();
    bar.pack_start(export_button);
    let out = zoom_button(
        "zoom-out-symbolic",
        &tr!("Zoom out"),
        canvas,
        zoom_label,
        DiagramCanvas::zoom_out,
    );
    let fit = zoom_button(
        "zoom-fit-best-symbolic",
        &tr!("Fit diagram to window"),
        canvas,
        zoom_label,
        DiagramCanvas::fit,
    );
    let into = zoom_button(
        "zoom-in-symbolic",
        &tr!("Zoom in"),
        canvas,
        zoom_label,
        DiagramCanvas::zoom_in,
    );
    bar.pack_end(&into);
    bar.pack_end(zoom_label);
    bar.pack_end(&fit);
    bar.pack_end(&out);
    bar
}

type TextFormat = (String, String, fn(&SchemaDiagram) -> String);

fn export_button(
    exported: &Rc<RefCell<SchemaDiagram>>,
    toasts: &adw::ToastOverlay,
    context: Option<DictionaryExportContext>,
) -> gtk::MenuButton {
    let popover = gtk::Popover::new();
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let formats: [TextFormat; 2] = [
        (tr!("Copy as Mermaid"), tr!("Mermaid text copied"), export::mermaid),
        (tr!("Copy as DBML"), tr!("DBML text copied"), export::dbml),
    ];
    for (label, confirmation, render) in formats {
        let choice = gtk::Button::with_label(&label);
        choice.add_css_class("flat");
        choice.connect_clicked({
            let (exported, toasts, popover) = (exported.clone(), toasts.downgrade(), popover.downgrade());
            move |button| {
                button.clipboard().set_text(&render(&exported.borrow()));
                if let Some(popover) = popover.upgrade() {
                    popover.popdown();
                }
                if let Some(toasts) = toasts.upgrade() {
                    toasts.add_toast(crate::ui::plain_text::toast(&confirmation));
                }
            }
        });
        choices.append(&choice);
    }
    if let Some(context) = context {
        append_dictionary_action(&choices, &popover, toasts, context);
    }
    popover.set_child(Some(&choices));
    let tooltip = tr!("Export diagram or dictionary metadata");
    let button = gtk::MenuButton::builder()
        .icon_name("document-export-symbolic")
        .tooltip_text(&tooltip)
        .popover(&popover)
        .sensitive(false)
        .build();
    button.update_property(&[gtk::accessible::Property::Label(&tooltip)]);
    button
}

fn append_dictionary_action(
    choices: &gtk::Box,
    popover: &gtk::Popover,
    toasts: &adw::ToastOverlay,
    context: DictionaryExportContext,
) {
    let choice = gtk::Button::with_label(&tr!("Export Data Dictionary…"));
    choice.add_css_class("flat");
    choice.connect_clicked({
        let (popover, toasts, choice) = (popover.downgrade(), toasts.downgrade(), choice.downgrade());
        move |_| {
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
            let Some(toast_overlay) = toasts.upgrade() else {
                return;
            };
            if let Some(choice) = choice.upgrade() {
                choice.set_sensitive(false);
            }
            let context = context.clone();
            let choice = choice.clone();
            let operation = context.token.child_token();
            let progress = dictionary_progress_toast(&toast_overlay, &operation);
            let toasts = toasts.clone();
            glib::spawn_future_local(async move {
                let result = load_dictionary(&context, &operation).await;
                progress.dismiss();
                if let Some(choice) = choice.upgrade() {
                    choice.set_sensitive(true);
                }
                if operation.is_cancelled() {
                    return;
                }
                let (Some(parent), Some(toasts)) = (context.parent.upgrade(), toasts.upgrade()) else {
                    return;
                };
                let result = match result {
                    Ok(result) => result,
                    Err(error) => {
                        let message = crate::services::error_text::driver_message(&error);
                        toasts.add_toast(crate::ui::plain_text::toast(&message));
                        return;
                    }
                };
                crate::ui::export_dialog::present(
                    &parent,
                    &toasts,
                    crate::ui::export_dialog::ExportRequest {
                        scope: crate::ui::export_dialog::ExportScope::DataDictionary,
                        paged: None,
                        result: Arc::new(result),
                        retained_page: None,
                        suggested_name: "data-dictionary".into(),
                        driver_id: context.driver_id,
                    },
                    &context.preferences,
                );
            });
        }
    });
    choices.append(&choice);
}

fn dictionary_progress_toast(toasts: &adw::ToastOverlay, operation: &CancellationToken) -> adw::Toast {
    let toast = crate::ui::plain_text::toast(&tr!("Reading database metadata…"));
    toast.set_timeout(0);
    toast.set_button_label(Some(&tr!("Cancel")));
    let operation = operation.clone();
    toast.connect_button_clicked(move |_| operation.cancel());
    toasts.add_toast(toast.clone());
    toast
}

async fn load_dictionary(
    context: &DictionaryExportContext,
    operation: &CancellationToken,
) -> Result<bookie_core::QueryResult, DriverError> {
    let control = crate::services::operation_control::bounded_with(context.timeout, operation.clone());
    let tables = match context.schema.as_deref() {
        Some(schema) => {
            context
                .connection
                .list_tables_in(&[schema.to_string()], &control)
                .await?
        }
        None => {
            crate::services::catalog::list_tables_excluding(
                context.connection.as_ref(),
                &context.hidden_schemas,
                &control,
            )
            .await?
        }
    };
    let tables = tables
        .into_iter()
        .filter(|table| {
            context
                .schema
                .as_deref()
                .is_none_or(|schema| table.schema.as_deref() == Some(schema))
        })
        .collect::<Vec<_>>();
    bookie_app_services::data_dictionary::load(
        context.connection.as_ref(),
        context.schema.as_deref(),
        &tables,
        context.capabilities,
        &control,
    )
    .await
}

fn zoom_button(
    icon: &str,
    tooltip: &str,
    canvas: &DiagramCanvas,
    zoom_label: &gtk::Label,
    action: fn(&DiagramCanvas),
) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    button.connect_clicked({
        let (canvas, zoom_label) = (canvas.downgrade(), zoom_label.downgrade());
        move |_| {
            let (Some(canvas), Some(zoom_label)) = (canvas.upgrade(), zoom_label.upgrade()) else {
                return;
            };
            action(&canvas);
            update_zoom(&canvas, &zoom_label);
        }
    });
    button
}

fn show_failure(ui: &Ui, token: &CancellationToken, error: &DriverError) {
    if token.is_cancelled() {
        ui.dialog.close();
        return;
    }
    ui.failure.set_title(&tr!("The diagram could not be built"));
    ui.failure.set_description(Some(&crate::ui::plain_text::escape_markup(
        &crate::services::error_text::driver_message(error),
    )));
    ui.stack.set_visible_child_name("failure");
}

fn show_diagram(ui: &Rc<Ui>, read: SchemaRead) {
    let notice = notice_text(&read);
    let diagram = SchemaDiagram::build(read.reads);
    if diagram.tables.is_empty() {
        ui.failure.set_title(&tr!("No tables to show"));
        ui.failure
            .set_description(Some(&tr!("This schema has no tables, or none could be read.")));
        ui.stack.set_visible_child_name("failure");
        return;
    }
    ui.banner.set_title(&notice);
    ui.banner.set_revealed(!notice.is_empty());
    fill_list(ui, &diagram);
    *ui.exported.borrow_mut() = diagram.clone();
    ui.export_button.set_sensitive(true);
    ui.canvas.set_diagram(diagram);
    ui.stack.set_visible_child_name("ready");
    update_zoom(&ui.canvas, &ui.zoom_label);
    schedule_zoom_update(ui);
    ui.canvas.grab_focus();
}

fn schedule_zoom_update(ui: &Rc<Ui>) {
    let canvas = ui.canvas.downgrade();
    let zoom_label = ui.zoom_label.downgrade();
    ui.canvas.add_tick_callback({
        move |_, _| {
            if let (Some(canvas), Some(zoom_label)) = (canvas.upgrade(), zoom_label.upgrade()) {
                update_zoom(&canvas, &zoom_label);
            }
            glib::ControlFlow::Break
        }
    });
}

fn update_zoom(canvas: &DiagramCanvas, zoom_label: &gtk::Label) {
    zoom_label.set_text(&format!("{}%", zoom::percent(canvas.zoom())));
}

fn fill_list(ui: &Rc<Ui>, diagram: &SchemaDiagram) {
    ui.list.remove_all();
    for (index, table) in diagram.tables.iter().enumerate() {
        let label = gtk::Label::builder()
            .label(&table.name)
            .xalign(0.0)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        label.set_ellipsize(relm4::gtk::pango::EllipsizeMode::End);
        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&label));
        row.update_property(&[
            gtk::accessible::Property::Label(&table.name),
            gtk::accessible::Property::Description(&table_description(diagram, index)),
        ]);
        ui.list.append(&row);
    }
    connect_selection(&ui.list, &ui.canvas);
}

fn connect_selection(list: &gtk::ListBox, canvas: &DiagramCanvas) {
    list.connect_row_selected({
        let canvas = canvas.downgrade();
        move |_, row| {
            let Some(canvas) = canvas.upgrade() else {
                return;
            };
            let index = row.and_then(|row| usize::try_from(row.index()).ok());
            canvas.select(index);
            if let Some(index) = index {
                canvas.reveal(index);
            }
        }
    });
    canvas.connect_selected({
        let list = list.downgrade();
        move |index| {
            let Some(list) = list.upgrade() else {
                return;
            };
            match index.and_then(|at| list.row_at_index(i32::try_from(at).unwrap_or(-1))) {
                Some(row) => list.select_row(Some(&row)),
                None => list.unselect_all(),
            }
        }
    });
}

fn notice_text(read: &SchemaRead) -> String {
    let mut parts = Vec::new();
    if read.metadata_limit_reached {
        parts.push(tr!(
            "The diagram is incomplete because schema metadata reached the 64 MiB estimated limit."
        ));
    } else if read.truncated() {
        parts.push(
            tr!("Showing the first {shown} of {total} tables.")
                .replace("{shown}", &read.reads.len().to_string())
                .replace("{total}", &read.total_tables.to_string()),
        );
    }
    if !read.skipped.is_empty() {
        parts.push(tr!("Skipped unreadable tables: {tables}.").replace("{tables}", &read.skipped.join(", ")));
    }
    if !read.foreign_keys_supported {
        parts.push(tr!(
            "This engine does not report foreign keys, so relations are not drawn."
        ));
    }
    parts.join(" ")
}

#[cfg(test)]
#[path = "dialog_lifetime_tests.rs"]
mod lifetime_tests;
