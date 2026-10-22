use std::rc::Rc;
use std::sync::Arc;

use relm4::adw::prelude::*;
use relm4::gtk::glib;
use relm4::{adw, gtk};

use crate::services::audit_view::{OperationRow, matches, operations, status_label};
use crate::services::database_service::DatabaseService;
use crate::tr;

const RECENT_RECORDS: usize = 1_000;

pub fn present(parent: &gtk::Window, database: &Arc<DatabaseService>) {
    let dialog = adw::Dialog::builder().title(tr!("Audit Log")).build();
    dialog.set_content_width(900);
    dialog.set_content_height(620);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    let search = gtk::SearchEntry::builder()
        .placeholder_text(tr!("Filter by operation id, connection, outcome or SQL"))
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(6)
        .build();
    toolbar.add_top_bar(&search);

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .valign(gtk::Align::Start)
        .build();
    let clamp = adw::Clamp::builder()
        .maximum_size(860)
        .child(&list)
        .margin_top(6)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    let scrolled = gtk::ScrolledWindow::builder().vexpand(true).child(&clamp).build();
    let status = adw::StatusPage::builder()
        .icon_name("document-open-recent-symbolic")
        .title(tr!("Reading the audit journal…"))
        .build();
    let stack = gtk::Stack::new();
    stack.add_named(&status, Some("status"));
    stack.add_named(&scrolled, Some("list"));
    toolbar.set_content(Some(&stack));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(parent));

    let database = database.clone();
    glib::spawn_future_local(async move {
        match database.recent_audit(RECENT_RECORDS).await {
            Ok(events) => {
                let rows = Rc::new(operations(events));
                if rows.is_empty() {
                    status.set_title(&tr!("No audited operations yet"));
                    return;
                }
                fill(&list, &rows, "");
                stack.set_visible_child_name("list");
                search.connect_search_changed(move |entry| fill(&list, &rows, &entry.text()));
            }
            Err(error) => {
                status.set_icon_name(Some("dialog-warning-symbolic"));
                status.set_title(&tr!("The audit journal could not be read"));
                status.set_description(Some(&error));
            }
        }
    });
}

fn fill(list: &gtk::ListBox, rows: &[OperationRow], query: &str) {
    list.remove_all();
    for row in rows.iter().filter(|row| matches(row, query)) {
        list.append(&operation_row(row));
    }
}

fn operation_row(row: &OperationRow) -> adw::ExpanderRow {
    let mut subtitle = vec![status_label(row.status), row.class.clone(), row.principal.clone()];
    if let Some(rows) = row.rows {
        subtitle.push(tr!("rows: {n}").replace("{n}", &rows.to_string()));
    }
    if let Some(ms) = row.duration_ms {
        subtitle.push(format!("{ms} ms"));
    }
    let expander = adw::ExpanderRow::builder()
        .title(glib::markup_escape_text(&format!(
            "{} · {}",
            row.started, row.connection
        )))
        .subtitle(glib::markup_escape_text(&subtitle.join(" · ")))
        .build();
    if row.status != tablepro_policy::AuditTerminalStatus::Succeeded {
        expander.add_css_class("warning");
    }
    expander.add_row(&detail(&tr!("Operation"), &row.operation_id.to_string(), false));
    expander.add_row(&detail(
        &tr!("Rule"),
        &format!("{} ({:?})", row.rule, row.approval),
        false,
    ));
    expander.add_row(&detail(&tr!("Statement"), &row.sql, true));
    if let Some(error) = &row.error {
        expander.add_row(&detail(&tr!("Error"), error, false));
    }
    expander
}

fn detail(title: &str, value: &str, monospace: bool) -> gtk::Box {
    let heading = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .css_classes(["caption-heading"])
        .build();
    let text = gtk::Label::builder()
        .label(value)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .selectable(true)
        .build();
    if monospace {
        text.add_css_class("monospace");
    }
    let container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(12)
        .margin_end(12)
        .build();
    container.append(&heading);
    container.append(&text);
    container
}
