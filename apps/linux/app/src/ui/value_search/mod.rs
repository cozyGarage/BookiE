use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use bookie_app_services::value_search::{
    ConnectionSource, SearchLimits, SearchOptions, SearchProgress, SearchReport, SearchRequest, TableTarget,
    run_search, validate_term,
};
use bookie_core::{DriverCapabilities, FilterSet, SearchMode};
use relm4::adw::prelude::*;
use relm4::{adw, gtk, gtk::glib};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::services::database_service::DatabaseService;
use crate::services::preferences::PreferencesStore;

mod results;

pub(crate) struct SearchDialogRequest {
    pub connection_id: Option<Uuid>,
    pub capabilities: DriverCapabilities,
    pub driver_id: String,
    pub scope: CancellationToken,
    pub hidden_schemas: Vec<String>,
}

type OpenTable = Rc<dyn Fn(Option<String>, String, FilterSet)>;

struct Ui {
    dialog: glib::WeakRef<adw::Dialog>,
    stack: gtk::Stack,
    term: adw::EntryRow,
    mode: adw::ComboRow,
    include_non_text: adw::SwitchRow,
    form_error: gtk::Label,
    progress_label: gtk::Label,
    results: gtk::Box,
    search_button: gtk::Button,
    cancel_button: gtk::Button,
    cancel: RefCell<Option<CancellationToken>>,
}

pub(crate) fn present(
    parent: &adw::ApplicationWindow,
    request: SearchDialogRequest,
    database: &Arc<DatabaseService>,
    preferences: &PreferencesStore,
    on_open: impl Fn(Option<String>, String, FilterSet) + 'static,
) {
    let Some(connection_id) = request.connection_id else {
        return notify(
            parent,
            &bookie_app_services::tr!("No active connection"),
            &bookie_app_services::tr!("Open a connection before searching it."),
        );
    };
    if !request.capabilities.supports_value_search {
        return notify(
            parent,
            &bookie_app_services::tr!("Search is not available"),
            &bookie_app_services::tr!("This connection type does not support searching values across tables."),
        );
    }
    let timeout_secs = crate::services::operation_control::timeout_for(preferences, database, request.connection_id);
    let (ui, dialog) = build_ui();
    let run = Rc::new(Run {
        ui,
        request,
        database: database.clone(),
        connection_id,
        timeout_secs,
        on_open: Rc::new(on_open),
    });
    connect_search(&run);
    connect_closed(&run, &dialog);
    crate::ui::leak_probe::present_tracked("value_search", &dialog, parent);
}

struct Run {
    ui: Ui,
    request: SearchDialogRequest,
    database: Arc<DatabaseService>,
    connection_id: Uuid,
    timeout_secs: u32,
    on_open: OpenTable,
}

fn build_ui() -> (Ui, adw::Dialog) {
    let (term, mode, include_non_text, group) = form_fields();
    let form_error = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .wrap(true)
        .css_classes(["error"])
        .visible(false)
        .build();
    let search_button = gtk::Button::builder()
        .label(bookie_app_services::tr!("Search"))
        .css_classes(["suggested-action"])
        .halign(gtk::Align::End)
        .build();
    let form = page_box(&[group.upcast_ref(), form_error.upcast_ref(), search_button.upcast_ref()]);
    let progress_label = gtk::Label::builder().wrap(true).build();
    let spinner = gtk::Spinner::new();
    spinner.start();
    let cancel_button = gtk::Button::with_label(&bookie_app_services::tr!("Cancel"));
    let progress = page_box(&[
        spinner.upcast_ref(),
        progress_label.upcast_ref(),
        cancel_button.upcast_ref(),
    ]);
    let results = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .build();
    let scroller = gtk::ScrolledWindow::builder().vexpand(true).child(&results).build();
    let stack = gtk::Stack::new();
    stack.add_named(&form, Some("form"));
    stack.add_named(&progress, Some("progress"));
    stack.add_named(&scroller, Some("results"));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&stack));
    let dialog = adw::Dialog::builder()
        .title(bookie_app_services::tr!("Search Database Values"))
        .content_width(520)
        .content_height(560)
        .child(&toolbar)
        .build();
    dialog.set_default_widget(Some(&search_button));
    let ui = Ui {
        dialog: dialog.downgrade(),
        stack,
        term,
        mode,
        include_non_text,
        form_error,
        progress_label,
        results,
        search_button,
        cancel_button,
        cancel: RefCell::new(None),
    };
    follow_mode(&ui);
    (ui, dialog)
}

fn form_fields() -> (adw::EntryRow, adw::ComboRow, adw::SwitchRow, adw::PreferencesGroup) {
    let term = adw::EntryRow::builder()
        .title(bookie_app_services::tr!("Value to find"))
        .build();
    let mode = adw::ComboRow::builder()
        .use_markup(false)
        .title(bookie_app_services::tr!("Match"))
        .model(&gtk::StringList::new(&[
            &bookie_app_services::tr!("Contains the text (ignoring case where the engine allows)"),
            &bookie_app_services::tr!("Equals the value exactly"),
        ]))
        .build();
    let include_non_text = adw::SwitchRow::builder()
        .title(bookie_app_services::tr!("Also match numbers and ids"))
        .subtitle(bookie_app_services::tr!(
            "Exact match only. Compares integer, decimal and UUID columns when the value fits their type."
        ))
        .sensitive(false)
        .build();
    let group = adw::PreferencesGroup::builder()
        .title(bookie_app_services::tr!("Search the database"))
        .description(bookie_app_services::tr!(
            "Searches the text columns of up to 200 tables, one table at a time. Nothing is changed."
        ))
        .build();
    group.add(&term);
    group.add(&mode);
    group.add(&include_non_text);
    (term, mode, include_non_text, group)
}

fn page_box(children: &[&gtk::Widget]) -> gtk::Box {
    let page = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    for child in children {
        page.append(*child);
    }
    page
}

fn follow_mode(ui: &Ui) {
    let include = ui.include_non_text.downgrade();
    ui.mode.connect_selected_notify(move |mode| {
        if let Some(include) = include.upgrade() {
            include.set_sensitive(mode.selected() == 1);
            if mode.selected() != 1 {
                include.set_active(false);
            }
        }
    });
}

fn connect_search(run: &Rc<Run>) {
    let for_search = Rc::downgrade(run);
    run.ui.search_button.connect_clicked(move |_| {
        if let Some(run) = for_search.upgrade() {
            start(&run);
        }
    });
    let for_cancel = Rc::downgrade(run);
    run.ui.cancel_button.connect_clicked(move |_| {
        if let Some(run) = for_cancel.upgrade() {
            run.cancel_search();
        }
    });
}

fn connect_closed(run: &Rc<Run>, dialog: &adw::Dialog) {
    let keep = run.clone();
    dialog.connect_closed(move |_| keep.cancel_search());
}

impl Run {
    fn cancel_search(&self) {
        if let Some(token) = self.ui.cancel.borrow().as_ref() {
            token.cancel();
        }
        self.ui
            .progress_label
            .set_label(&bookie_app_services::tr!("Cancelling…"));
    }

    fn options(&self) -> SearchOptions {
        let exact = self.ui.mode.selected() == 1;
        SearchOptions {
            mode: if exact { SearchMode::Exact } else { SearchMode::Contains },
            include_non_text: exact && self.ui.include_non_text.is_active(),
        }
    }

    fn show_progress(&self, progress: &SearchProgress) {
        self.ui.progress_label.set_label(
            &bookie_app_services::tr!("Searching {table} ({done} of {total})")
                .replace("{table}", &progress.current)
                .replace("{done}", &(progress.searched + 1).to_string())
                .replace("{total}", &progress.total.to_string()),
        );
    }
}

fn start(run: &Rc<Run>) {
    let term = run.ui.term.text().to_string();
    if let Err(error) = validate_term(&term) {
        run.ui.form_error.set_label(&error.to_string());
        run.ui.form_error.set_visible(true);
        return;
    }
    let Some(connection) = run.database.get(run.connection_id) else {
        return;
    };
    run.ui.form_error.set_visible(false);
    let token = run.request.scope.child_token();
    run.ui.cancel.replace(Some(token.clone()));
    run.ui
        .progress_label
        .set_label(&bookie_app_services::tr!("Listing tables…"));
    run.ui.stack.set_visible_child_name("progress");
    let run = run.clone();
    glib::spawn_future_local(async move {
        let options = run.options();
        let control = crate::services::operation_control::bounded_with(run.timeout_secs, token.child_token());
        let listed =
            crate::services::catalog::list_tables_excluding(connection.as_ref(), &run.request.hidden_schemas, &control)
                .await;
        let tables = match listed {
            Ok(tables) => tables,
            Err(error) => return show_failure(&run, &error),
        };
        let targets: Vec<TableTarget> = tables
            .into_iter()
            .map(|table| TableTarget {
                schema: table.schema,
                name: table.name,
            })
            .collect();
        let outcome = run_search(
            &ConnectionSource(connection.as_ref()),
            SearchRequest {
                driver_id: &run.request.driver_id,
                tables: targets,
                term: &term,
                options,
                limits: SearchLimits::default(),
            },
            &token,
            |progress| run.show_progress(&progress),
        )
        .await;
        match outcome {
            Ok(report) => show_report(&run, &report, &term, options.mode),
            Err(error) => {
                run.ui.form_error.set_label(&error.to_string());
                run.ui.form_error.set_visible(true);
                run.ui.stack.set_visible_child_name("form");
            }
        }
    });
}

fn show_failure(run: &Rc<Run>, error: &bookie_core::DriverError) {
    clear(&run.ui.results);
    let message = if matches!(error, bookie_core::DriverError::Cancelled) {
        bookie_app_services::tr!("The search was cancelled.")
    } else {
        crate::services::error_text::driver_message(error)
    };
    run.ui.results.append(&results::note_label(&message));
    run.ui.stack.set_visible_child_name("results");
}

fn show_report(run: &Rc<Run>, report: &SearchReport, term: &str, mode: SearchMode) {
    clear(&run.ui.results);
    let dialog = run.ui.dialog.clone();
    let on_open = run.on_open.clone();
    let open: results::OpenHit = Rc::new(move |schema, table, filter| {
        if let Some(dialog) = dialog.upgrade() {
            dialog.force_close();
        }
        on_open(schema, table, filter);
    });
    results::fill(&run.ui.results, report, term, mode, &open);
    run.ui.stack.set_visible_child_name("results");
}

fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn notify(parent: &adw::ApplicationWindow, title: &str, body: &str) {
    let alert = adw::AlertDialog::new(Some(title), Some(body));
    alert.add_response("ok", &bookie_app_services::tr!("OK"));
    alert.present(Some(parent));
}
