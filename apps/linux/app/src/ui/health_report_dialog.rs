use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use tokio_util::sync::CancellationToken;

use bookie_app_services::health_report::{HealthRow, HealthRows, run_health_report};
use bookie_app_services::tr;
use bookie_core::HealthReport;

use crate::services::database_service::DatabaseService;
use crate::services::preferences::PreferencesStore;

pub(crate) struct HealthReportSource {
    pub(crate) connection: uuid::Uuid,
    pub(crate) database: Arc<DatabaseService>,
    pub(crate) preferences: PreferencesStore,
    pub(crate) hidden_schemas: Vec<String>,
    pub(crate) open_in_editor: Rc<dyn Fn(String)>,
}

struct Panel {
    source: HealthReportSource,
    list: gtk::ListBox,
    status: gtk::Label,
    in_flight: RefCell<CancellationToken>,
}

fn report_label(report: HealthReport) -> String {
    match report {
        HealthReport::TableVacuum => tr!("Dead rows and vacuum"),
        HealthReport::EstimatedBloat => tr!("Estimated bloat"),
        HealthReport::UnusedIndexes => tr!("Unused indexes"),
        HealthReport::DuplicateIndexes => tr!("Duplicate indexes"),
        HealthReport::SeqScanHeavy => tr!("Sequential scans"),
    }
}

fn report_hint(report: HealthReport) -> String {
    match report {
        HealthReport::TableVacuum => {
            tr!("Tables with the most dead rows, with their size and last vacuum and analyze.")
        }
        HealthReport::EstimatedBloat => {
            tr!("A rough estimate from the share of dead rows in tables of 1 MiB or more. It is not a measurement.")
        }
        HealthReport::UnusedIndexes => tr!(
            "Non-unique indexes never scanned since the statistics were last reset. Check how long the statistics have run before dropping one."
        ),
        HealthReport::DuplicateIndexes => {
            tr!("Indexes whose leading columns are already covered by another index on the same table.")
        }
        HealthReport::SeqScanHeavy => {
            tr!("Tables of 10,000 rows or more that are read by sequential scans more often than by an index.")
        }
    }
}

pub(crate) fn present(parent: &gtk::Window, source: HealthReportSource) {
    let dialog = adw::Dialog::builder().title(tr!("Table and Index Health")).build();
    dialog.set_content_width(900);
    dialog.set_content_height(600);
    let panel = Rc::new(Panel {
        source,
        list: gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).build(),
        status: gtk::Label::builder()
            .label(tr!(
                "Choose a report above. Reports only read statistics and never change the database."
            ))
            .xalign(0.0)
            .wrap(true)
            .margin_start(12)
            .margin_end(12)
            .margin_bottom(8)
            .build(),
        in_flight: RefCell::new(CancellationToken::new()),
    });
    panel.list.add_css_class("boxed-list");
    panel.status.add_css_class("dim-label");
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&content(&panel)));
    dialog.set_child(Some(&toolbar));
    let closing = panel.clone();
    dialog.connect_closed(move |_| closing.in_flight.borrow().cancel());
    dialog.present(Some(parent));
}

fn content(panel: &Rc<Panel>) -> gtk::Box {
    let buttons = gtk::Box::builder()
        .spacing(8)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    for report in HealthReport::ALL {
        let button = gtk::Button::with_label(&report_label(report));
        button.set_tooltip_text(Some(&report_hint(report)));
        let panel = panel.clone();
        button.connect_clicked(move |_| start(&panel, report));
        buttons.append(&button);
    }
    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .margin_start(12)
        .margin_end(12)
        .child(&panel.list)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.append(&buttons);
    body.append(&scroller);
    body.append(&panel.status);
    body
}

fn start(panel: &Rc<Panel>, report: HealthReport) {
    let source = &panel.source;
    let Some(connection) = source.database.get(source.connection) else {
        panel.status.set_text(&tr!("Connection closed."));
        return;
    };
    panel.status.set_text(&tr!("Running…"));
    let token = bookie_app_services::request_generation::replace_in_flight(&panel.in_flight);
    let timeout_secs =
        crate::services::operation_control::timeout_for(&source.preferences, &source.database, Some(source.connection));
    let hidden = source.hidden_schemas.clone();
    let panel = panel.clone();
    glib::spawn_future_local(async move {
        let control = crate::services::operation_control::bounded_with(timeout_secs, token.clone());
        let outcome = run_health_report(connection.as_ref(), report, &hidden, &control).await;
        if token.is_cancelled() {
            return;
        }
        match outcome {
            Ok(rows) => show_rows(&panel, report, &rows),
            Err(error) => {
                clear(&panel.list);
                panel
                    .status
                    .set_text(&crate::services::error_text::driver_message(&error));
            }
        }
    });
}

fn show_rows(panel: &Panel, report: HealthReport, rows: &HealthRows) {
    clear(&panel.list);
    for row in &rows.rows {
        panel.list.append(&row_widget(row, &panel.source.open_in_editor));
    }
    panel.status.set_text(&rows_status(report, rows));
}

fn rows_status(report: HealthReport, rows: &HealthRows) -> String {
    if rows.rows.is_empty() {
        return tr!("Nothing to report. {hint}").replace("{hint}", &report_hint(report));
    }
    let shown = tr!("{count} rows. {hint}")
        .replace("{count}", &rows.rows.len().to_string())
        .replace("{hint}", &report_hint(report));
    if rows.may_have_more {
        return format!("{shown} {}", tr!("Only the top rows are shown."));
    }
    shown
}

fn clear(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn row_widget(row: &HealthRow, open_in_editor: &Rc<dyn Fn(String)>) -> adw::ActionRow {
    let widget = adw::ActionRow::builder()
        .title(&row.title)
        .subtitle(&row.summary)
        .use_markup(false)
        .subtitle_lines(0)
        .subtitle_selectable(true)
        .build();
    let Some(sql) = row.diagnostic_sql.clone() else {
        return widget;
    };
    let copy = gtk::Button::from_icon_name("edit-copy-symbolic");
    copy.set_valign(gtk::Align::Center);
    copy.set_tooltip_text(Some(&tr!("Copy diagnostic SQL")));
    copy.update_property(&[gtk::accessible::Property::Label(&tr!("Copy diagnostic SQL"))]);
    let copied = sql.clone();
    copy.connect_clicked(move |button| button.clipboard().set_text(&copied));
    let open = gtk::Button::from_icon_name("text-editor-symbolic");
    open.set_valign(gtk::Align::Center);
    open.set_tooltip_text(Some(&tr!("Open diagnostic SQL in an editor tab")));
    open.update_property(&[gtk::accessible::Property::Label(&tr!(
        "Open diagnostic SQL in an editor tab"
    ))]);
    let opener = open_in_editor.clone();
    open.connect_clicked(move |_| opener(sql.clone()));
    widget.add_suffix(&copy);
    widget.add_suffix(&open);
    widget
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_report_has_its_own_label_and_hint() {
        let labels: Vec<String> = HealthReport::ALL.iter().map(|report| report_label(*report)).collect();
        let mut unique = labels.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), labels.len());
        assert!(HealthReport::ALL.iter().all(|report| !report_hint(*report).is_empty()));
    }

    #[test]
    fn the_status_says_when_the_list_is_cut_at_the_limit() {
        let page = |count: usize, may_have_more: bool| HealthRows {
            rows: (0..count)
                .map(|index| HealthRow {
                    title: format!("s.t{index}"),
                    summary: String::new(),
                    diagnostic_sql: None,
                })
                .collect(),
            may_have_more,
        };
        assert!(rows_status(HealthReport::UnusedIndexes, &page(0, false)).starts_with("Nothing to report."));
        assert!(rows_status(HealthReport::UnusedIndexes, &page(3, false)).starts_with("3 rows."));
        assert!(rows_status(HealthReport::UnusedIndexes, &page(50, true)).ends_with("Only the top rows are shown."));
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn a_row_with_a_diagnostic_offers_copy_and_open_and_a_row_without_offers_neither() {
        gtk::init().expect("display");
        adw::init().expect("libadwaita");
        let opened = Rc::new(RefCell::new(Vec::<String>::new()));
        let sink = opened.clone();
        let open: Rc<dyn Fn(String)> = Rc::new(move |sql| sink.borrow_mut().push(sql));
        let with_sql = HealthRow {
            title: "public.orders".into(),
            summary: "Dead rows: 4".into(),
            diagnostic_sql: Some("VACUUM (ANALYZE) \"public\".\"orders\"".into()),
        };
        let widget = row_widget(&with_sql, &open);
        let buttons = descendant_buttons(widget.upcast_ref());
        assert_eq!(buttons.len(), 2);
        buttons[1].emit_clicked();
        assert_eq!(
            *opened.borrow(),
            vec!["VACUUM (ANALYZE) \"public\".\"orders\"".to_owned()]
        );
        let without = HealthRow {
            diagnostic_sql: None,
            ..with_sql
        };
        assert!(descendant_buttons(row_widget(&without, &open).upcast_ref()).is_empty());
    }

    fn descendant_buttons(widget: &gtk::Widget) -> Vec<gtk::Button> {
        let mut found = Vec::new();
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(button) = current.downcast_ref::<gtk::Button>() {
                found.push(button.clone());
            }
            found.extend(descendant_buttons(&current));
            child = current.next_sibling();
        }
        found
    }
}
