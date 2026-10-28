use std::rc::Rc;
use std::sync::Arc;

use relm4::adw::prelude::*;
use relm4::{adw, gtk, gtk::glib};
use tokio_util::sync::CancellationToken;

use tablepro_app_services::er_diagram::read::{SchemaRead, read_schema};
use tablepro_app_services::er_diagram::{SchemaDiagram, zoom};
use tablepro_core::{DriverCapabilities, DriverError};

use super::canvas::DiagramCanvas;
use super::describe::{diagram_label, table_description};
use crate::services::database_service::DatabaseService;
use crate::services::preferences::PreferencesStore;
use crate::tr;

struct Ui {
    dialog: adw::Dialog,
    stack: gtk::Stack,
    progress: gtk::Label,
    failure: adw::StatusPage,
    banner: adw::Banner,
    list: gtk::ListBox,
    canvas: DiagramCanvas,
    zoom_label: gtk::Label,
}

pub fn present(
    parent: &gtk::Window,
    schema: Option<String>,
    connection_id: Option<uuid::Uuid>,
    database: &Arc<DatabaseService>,
    preferences: &PreferencesStore,
    capabilities: DriverCapabilities,
    scope: CancellationToken,
) {
    let Some(connection) = connection_id.and_then(|id| database.get(id)) else {
        let alert = adw::AlertDialog::new(
            Some(&tr!("No active connection")),
            Some(&tr!("Open a connection before showing its diagram.")),
        );
        alert.add_response("ok", &tr!("OK"));
        alert.present(Some(parent));
        return;
    };
    let token = scope.child_token();
    let ui = Rc::new(build(&diagram_label(schema.as_deref()), &token));
    ui.dialog.present(Some(parent));
    let timeout = crate::services::operation_control::timeout_for(preferences, database, connection_id);
    glib::spawn_future_local(async move {
        let control = crate::services::operation_control::bounded_with(timeout, token.clone());
        let tables = match connection.list_tables_controlled(&control).await {
            Ok(tables) => tables
                .into_iter()
                .filter(|table| schema.is_none() || table.schema == schema)
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
            schema.as_deref(),
            &tables,
            capabilities,
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

fn build(title: &str, token: &CancellationToken) -> Ui {
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
    let banner = adw::Banner::builder().revealed(false).build();
    let stack = gtk::Stack::new();
    let progress = gtk::Label::new(Some(&tr!("Reading tables…")));
    let failure = adw::StatusPage::builder().icon_name("dialog-warning-symbolic").build();
    stack.add_named(&loading_page(&progress, &dialog, token), Some("loading"));
    stack.add_named(&failure, Some("failure"));
    stack.add_named(&ready_page(&banner, &list, &canvas), Some("ready"));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header(&canvas, &zoom_label));
    toolbar.set_content(Some(&stack));
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

fn header(canvas: &DiagramCanvas, zoom_label: &gtk::Label) -> adw::HeaderBar {
    let bar = adw::HeaderBar::new();
    let out = zoom_button("zoom-out-symbolic", &tr!("Zoom out"), canvas, DiagramCanvas::zoom_out);
    let fit = zoom_button(
        "zoom-fit-best-symbolic",
        &tr!("Fit diagram to window"),
        canvas,
        DiagramCanvas::fit,
    );
    let into = zoom_button("zoom-in-symbolic", &tr!("Zoom in"), canvas, DiagramCanvas::zoom_in);
    bar.pack_end(&into);
    bar.pack_end(zoom_label);
    bar.pack_end(&fit);
    bar.pack_end(&out);
    bar
}

fn zoom_button(icon: &str, tooltip: &str, canvas: &DiagramCanvas, action: fn(&DiagramCanvas)) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    button.connect_clicked({
        let canvas = canvas.clone();
        move |_| action(&canvas)
    });
    button
}

fn show_failure(ui: &Ui, token: &CancellationToken, error: &DriverError) {
    if token.is_cancelled() {
        ui.dialog.close();
        return;
    }
    ui.failure.set_title(&tr!("The diagram could not be built"));
    ui.failure
        .set_description(Some(&crate::services::error_text::driver_message(error)));
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
    ui.canvas.set_diagram(diagram);
    ui.stack.set_visible_child_name("ready");
    update_zoom(ui);
    ui.canvas.add_tick_callback({
        let ui = ui.clone();
        move |_, _| {
            update_zoom(&ui);
            glib::ControlFlow::Continue
        }
    });
    ui.canvas.grab_focus();
}

fn update_zoom(ui: &Ui) {
    ui.zoom_label.set_text(&format!("{}%", zoom::percent(ui.canvas.zoom())));
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
    ui.list.connect_row_selected({
        let canvas = ui.canvas.clone();
        move |_, row| {
            let index = row.and_then(|row| usize::try_from(row.index()).ok());
            canvas.select(index);
            if let Some(index) = index {
                canvas.reveal(index);
            }
        }
    });
    ui.canvas.connect_selected({
        let list = ui.list.clone();
        move |index| match index.and_then(|at| list.row_at_index(i32::try_from(at).unwrap_or(-1))) {
            Some(row) => list.select_row(Some(&row)),
            None => list.unselect_all(),
        }
    });
}

fn notice_text(read: &SchemaRead) -> String {
    let mut parts = Vec::new();
    if read.truncated() {
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
