use std::path::PathBuf;

use crate::services::csv_import_mappings::{CsvMappingScope, CsvMappingStore};
use crate::ui::progress_dialog::CancellableDialog;
use bookie_core::export::CsvDelimiter;
use bookie_core::import::{
    CsvFormat, CsvImportOptions, CsvSheet, INFER_SAMPLE_ROWS, MAX_PREVIEW_ROWS, infer_columns, read_csv,
    suggest_mapping,
};
use bookie_core::import::{CsvImportEncoding, CsvImportQuote};
use bookie_core::sql_ddl::DraftColumn;
use bookie_core::{ColumnInfo, DriverCapabilities, sql_dialect};
use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use tokio_util::sync::CancellationToken;

mod preview_queue;
mod write_mode;

/// Whether the rows go into a table that is already there, or into one
/// the import creates first.
pub(crate) enum ImportMode {
    ExistingTable {
        capabilities: DriverCapabilities,
        table: String,
        columns: Vec<ColumnInfo>,
        keys: Vec<bookie_core::import::ConflictKey>,
    },
    NewTable {
        driver_id: String,
        capabilities: DriverCapabilities,
        suggested_table: String,
    },
}

/// What the app read from the file before it asked the user anything.
pub(crate) struct ImportRequest {
    pub schema: Option<String>,
    pub path: PathBuf,
    pub bytes: std::sync::Arc<[u8]>,
    pub format: CsvFormat,
    pub mode: ImportMode,
    pub mapping_store: Option<CsvMappingStore>,
    pub mapping_scope: Option<CsvMappingScope>,
    pub more_files: Vec<PathBuf>,
}

/// What the user settled on. The app re-reads the whole file with these
/// options; the dialog only ever read a preview.
#[derive(Debug, Clone)]
pub(crate) struct ImportChoice {
    pub schema: Option<String>,
    pub table: String,
    pub bytes: std::sync::Arc<[u8]>,
    pub options: CsvImportOptions,
    pub mapping: Vec<Option<usize>>,
    pub columns: Vec<ColumnInfo>,
    pub capabilities: DriverCapabilities,
    /// The table to create first, when the import is making one. The
    /// CREATE is its own governed statement, run before any row.
    pub create: Option<Vec<DraftColumn>>,
    pub skip_rejected_rows: bool,
    pub write: bookie_core::import::WriteMode,
    pub source: PathBuf,
    pub more_files: Vec<PathBuf>,
}

fn skip_rejected_group() -> (adw::SwitchRow, adw::PreferencesGroup) {
    let skip_rejected = adw::SwitchRow::builder()
        .title(bookie_app_services::tr!("Skip rows the database rejects"))
        .subtitle(bookie_app_services::tr!(
            "Off: the first rejected row stops the import. On: rejected rows are skipped and listed when the import finishes."
        ))
        .build();
    let errors = adw::PreferencesGroup::builder()
        .title(bookie_app_services::tr!("Errors"))
        .build();
    errors.add(&skip_rejected);
    (skip_rejected, errors)
}

fn existing_write_mode(mode: &ImportMode) -> Option<write_mode::WriteModeSection> {
    let ImportMode::ExistingTable { capabilities, keys, .. } = mode else {
        return None;
    };
    write_mode::WriteModeSection::new(capabilities, keys)
}

const SKIP_LABEL: &str = "Skip";

pub(crate) fn present(
    parent: &adw::ApplicationWindow,
    request: ImportRequest,
    on_start: impl Fn(ImportChoice) + 'static,
) {
    let page = adw::PreferencesPage::new();
    let file = FileOptions::new(&request);
    page.add(&file.group);
    let preview_status = gtk::Label::builder()
        .label(bookie_app_services::tr!("Preparing CSV preview…"))
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();
    let preview_group = adw::PreferencesGroup::new();
    preview_group.add(&preview_status);
    page.add(&preview_group);
    let columns = ColumnSection::new(&request);
    page.add(columns.group());
    let write_mode = existing_write_mode(&request.mode);
    if let Some(section) = &write_mode {
        page.add(section.group());
    }
    if let Some(notice) = write_mode::queue_notice(request.more_files.len()) {
        page.add(&notice);
    }
    let (skip_rejected, errors) = skip_rejected_group();
    page.add(&errors);
    let start_button = gtk::Button::builder().label(bookie_app_services::tr!("Import")).build();
    start_button.add_css_class("suggested-action");
    start_button.set_sensitive(false);
    let state = std::rc::Rc::new(State {
        request,
        file,
        columns,
        write_mode,
        skip_rejected,
        preview_status,
        start_button: start_button.downgrade(),
        preview_queue: std::cell::RefCell::new(preview_queue::PreviewQueue::default()),
        ready_generation: std::cell::Cell::new(None),
        auto_detect_encoding: std::cell::Cell::new(true),
        suppress_refresh: std::cell::Cell::new(false),
    });
    connect_refresh(&state);
    state.refresh();
    let dialog = build_dialog(&page, &start_button);
    let dialog_weak = dialog.downgrade();
    let weak_state = std::rc::Rc::downgrade(&state);
    dialog.connect_closed(move |_| {
        if let Some(state) = weak_state.upgrade() {
            state.close_preview();
        }
    });
    start_button.connect_clicked(move |_| {
        let Some(dialog) = dialog_weak.upgrade() else {
            return;
        };
        let Some(choice) = state.choice() else {
            return;
        };
        dialog.close();
        on_start(choice);
    });
    crate::ui::leak_probe::present_tracked("import_csv", &dialog, parent);
}

struct State {
    request: ImportRequest,
    file: FileOptions,
    columns: ColumnSection,
    write_mode: Option<write_mode::WriteModeSection>,
    skip_rejected: adw::SwitchRow,
    preview_status: gtk::Label,
    start_button: gtk::glib::WeakRef<gtk::Button>,
    preview_queue: std::cell::RefCell<preview_queue::PreviewQueue>,
    ready_generation: std::cell::Cell<Option<u64>>,
    auto_detect_encoding: std::cell::Cell<bool>,
    suppress_refresh: std::cell::Cell<bool>,
}

impl State {
    fn refresh(self: &std::rc::Rc<Self>) {
        if self.suppress_refresh.get() {
            return;
        }
        let options = self.file.options();
        self.ready_generation.set(None);
        self.set_start_sensitive(false);
        self.preview_status
            .set_text(&bookie_app_services::tr!("Preparing CSV preview…"));
        self.preview_status.remove_css_class("error");
        self.preview_status.add_css_class("dim-label");
        let auto_detect = self.auto_detect_encoding.get();
        let work = self.preview_queue.borrow_mut().request(options, auto_detect);
        if let Some(work) = work {
            start_preview(self, work);
        }
    }

    fn close_preview(&self) {
        self.preview_queue.borrow_mut().close();
        self.ready_generation.set(None);
    }

    fn set_start_sensitive(&self, sensitive: bool) {
        if let Some(start_button) = self.start_button.upgrade() {
            start_button.set_sensitive(sensitive);
        }
    }

    /// The user's answer, or nothing when a name they typed cannot be an
    /// identifier. The rows say which; the import does not start.
    fn choice(&self) -> Option<ImportChoice> {
        let generation = self.preview_queue.borrow().current_generation();
        if !self
            .ready_generation
            .get()
            .is_some_and(|ready| self.preview_queue.borrow().is_ready(ready) && ready == generation)
        {
            return None;
        }
        let options = self.file.options();
        let (table, columns, mapping, create) = self.columns.resolve()?;
        let write = match &self.write_mode {
            Some(section) => section.selection()?,
            None => bookie_core::import::WriteMode::Insert,
        };
        Some(ImportChoice {
            schema: self.request.schema.clone(),
            table,
            bytes: self.request.bytes.clone(),
            options,
            mapping,
            columns,
            capabilities: match &self.request.mode {
                ImportMode::ExistingTable { capabilities, .. } | ImportMode::NewTable { capabilities, .. } => {
                    *capabilities
                }
            },
            create,
            skip_rejected_rows: self.skip_rejected.is_active(),
            write,
            source: self.request.path.clone(),
            more_files: self.request.more_files.clone(),
        })
    }
}

fn start_preview(state: &std::rc::Rc<State>, work: preview_queue::PreviewWork) {
    let bytes = state.request.bytes.clone();
    let weak_state = std::rc::Rc::downgrade(state);
    let generation = work.generation;
    let auto_detect = work.auto_detect;
    let options = work.options;
    let worker_options = options.clone();
    gtk::glib::spawn_future_local(async move {
        let result = gtk::gio::spawn_blocking(move || {
            let encoding = auto_detect.then(|| starting_encoding(&bytes));
            let mut options = worker_options;
            if let Some(index) = encoding {
                options.encoding = CsvImportEncoding::ALL[index as usize];
            }
            let sheet = read_csv(&bytes, &options, Some(MAX_PREVIEW_ROWS.max(INFER_SAMPLE_ROWS)));
            (encoding, sheet)
        })
        .await;
        let Some(state) = weak_state.upgrade() else {
            return;
        };
        let result = result
            .map_err(|_| ())
            .map(|(encoding, sheet)| (encoding, sheet.map_err(|_| ())));
        let completion = state.preview_queue.borrow_mut().finish(generation);
        if completion.apply {
            state.finish_preview(result, &options, auto_detect);
        }
        if let Some(next) = completion.next {
            start_preview(&state, next);
        }
    });
}

impl State {
    fn finish_preview(
        &self,
        result: Result<(Option<u32>, Result<CsvSheet, ()>), ()>,
        options: &CsvImportOptions,
        auto_detect: bool,
    ) {
        let Ok((encoding, sheet)) = result else {
            self.show_preview_error();
            return;
        };
        if auto_detect {
            self.auto_detect_encoding.set(false);
            if let Some(encoding) = encoding {
                self.suppress_refresh.set(true);
                self.file.encoding.set_selected(encoding);
                self.suppress_refresh.set(false);
            }
        }
        let Ok(sheet) = sheet else {
            self.show_preview_error();
            return;
        };
        let options = if auto_detect {
            self.file.options()
        } else {
            options.clone()
        };
        self.columns.refresh(&sheet, &options);
        let generation = self.preview_queue.borrow().current_generation();
        self.ready_generation.set(Some(generation));
        self.preview_status
            .set_text(&bookie_app_services::tr!("CSV preview ready."));
        self.preview_status.remove_css_class("dim-label");
        self.set_start_sensitive(self.columns.resolve().is_some());
    }

    fn show_preview_error(&self) {
        self.preview_status.set_text(&bookie_app_services::tr!(
            "The CSV preview could not be read with these settings."
        ));
        self.preview_status.remove_css_class("dim-label");
        self.preview_status.add_css_class("error");
        self.set_start_sensitive(false);
    }
}

fn connect_refresh(state: &std::rc::Rc<State>) {
    let for_delimiter = std::rc::Rc::downgrade(state);
    state.file.delimiter.connect_selected_notify(move |_| {
        if let Some(state) = for_delimiter.upgrade() {
            state.refresh();
        }
    });
    let for_quote = std::rc::Rc::downgrade(state);
    state.file.quote.connect_selected_notify(move |_| {
        if let Some(state) = for_quote.upgrade() {
            state.refresh();
        }
    });
    let for_encoding = std::rc::Rc::downgrade(state);
    state.file.encoding.connect_selected_notify(move |_| {
        if let Some(state) = for_encoding.upgrade() {
            state.auto_detect_encoding.set(false);
            state.refresh();
        }
    });
    let for_backslash = std::rc::Rc::downgrade(state);
    state.file.backslash.connect_active_notify(move |_| {
        if let Some(state) = for_backslash.upgrade() {
            state.refresh();
        }
    });
    let for_header = std::rc::Rc::downgrade(state);
    state.file.header.connect_active_notify(move |_| {
        if let Some(state) = for_header.upgrade() {
            state.refresh();
        }
    });
}

struct FileOptions {
    group: adw::PreferencesGroup,
    delimiter: adw::ComboRow,
    quote: adw::ComboRow,
    backslash: adw::SwitchRow,
    encoding: adw::ComboRow,
    header: adw::SwitchRow,
    null_marker: adw::EntryRow,
}

pub(crate) fn file_group(path: &std::path::Path) -> adw::PreferencesGroup {
    adw::PreferencesGroup::builder()
        .title(bookie_app_services::tr!("File"))
        .description(crate::ui::plain_text::escape_markup(&path.display().to_string()))
        .build()
}

impl FileOptions {
    fn new(request: &ImportRequest) -> Self {
        let group = file_group(&request.path);
        let delimiter = delimiter_row(request.format.delimiter);
        let header = adw::SwitchRow::builder()
            .title(bookie_app_services::tr!("First row names the columns"))
            .active(request.format.has_header)
            .build();
        let null_marker = adw::EntryRow::builder()
            .title(bookie_app_services::tr!("Text that means NULL"))
            .build();
        let quote = choice_row(
            &bookie_app_services::tr!("Quote character"),
            &["Double quote (\")", "Single quote (')", "None"],
        );
        let backslash = adw::SwitchRow::builder()
            .title(bookie_app_services::tr!("Backslash escapes a quote"))
            .build();
        let encoding = choice_row(
            &bookie_app_services::tr!("Encoding"),
            &["UTF-8", "Windows-1252", "UTF-16 LE", "UTF-16 BE"],
        );
        group.add(&delimiter);
        group.add(&quote);
        group.add(&backslash);
        group.add(&encoding);
        group.add(&header);
        group.add(&null_marker);
        Self {
            group,
            delimiter,
            quote,
            backslash,
            encoding,
            header,
            null_marker,
        }
    }

    fn options(&self) -> CsvImportOptions {
        CsvImportOptions {
            delimiter: selected_delimiter(self.delimiter.selected()),
            quote: CsvImportQuote::ALL
                .get(self.quote.selected() as usize)
                .copied()
                .unwrap_or_default(),
            backslash_escape: self.backslash.is_active(),
            encoding: CsvImportEncoding::ALL
                .get(self.encoding.selected() as usize)
                .copied()
                .unwrap_or_default(),
            has_header: self.header.is_active(),
            null_marker: self.null_marker.text().to_string(),
        }
    }
}

type ResolvedTarget = (String, Vec<ColumnInfo>, Vec<Option<usize>>, Option<Vec<DraftColumn>>);

enum ColumnSection {
    Existing(ExistingColumns),
    New(NewColumns),
}

impl ColumnSection {
    fn new(request: &ImportRequest) -> Self {
        match &request.mode {
            ImportMode::ExistingTable {
                capabilities,
                table,
                columns,
                ..
            } => Self::Existing(ExistingColumns::new(
                table,
                capabilities,
                columns,
                request.mapping_store.clone(),
                request.mapping_scope.clone(),
            )),
            ImportMode::NewTable {
                driver_id,
                suggested_table,
                ..
            } => Self::New(NewColumns::new(driver_id, suggested_table)),
        }
    }

    fn group(&self) -> &adw::PreferencesGroup {
        match self {
            Self::Existing(existing) => &existing.group,
            Self::New(new) => &new.group,
        }
    }

    fn refresh(&self, sheet: &CsvSheet, options: &CsvImportOptions) {
        match self {
            Self::Existing(existing) => existing.refresh(sheet, options.has_header),
            Self::New(new) => new.refresh(sheet, options),
        }
    }

    fn resolve(&self) -> Option<ResolvedTarget> {
        match self {
            Self::Existing(existing) => Some((
                existing.table.clone(),
                existing.columns.clone(),
                existing.mapping(),
                None,
            )),
            Self::New(new) => new.resolve(),
        }
    }
}

struct ExistingColumns {
    group: adw::PreferencesGroup,
    capabilities: DriverCapabilities,
    table: String,
    columns: Vec<ColumnInfo>,
    rows: Vec<adw::ComboRow>,
    headers: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    has_header: std::rc::Rc<std::cell::Cell<bool>>,
    initialized: std::cell::Cell<bool>,
}

impl ExistingColumns {
    fn new(
        table: &str,
        capabilities: &DriverCapabilities,
        columns: &[ColumnInfo],
        store: Option<CsvMappingStore>,
        scope: Option<CsvMappingScope>,
    ) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(bookie_app_services::tr!("Columns"))
            .description(bookie_app_services::tr!(
                "Generated columns stay on Skip and are filled by the database. Other columns left on Skip are not written, so their database defaults apply."
            ))
            .build();
        let rows: Vec<adw::ComboRow> = columns
            .iter()
            .map(|column| {
                let row = adw::ComboRow::builder()
                    .use_markup(false)
                    .title(&column.name)
                    .subtitle(column_subtitle(*capabilities, column))
                    .build();
                row.set_sensitive(!server_managed(*capabilities, column));
                row
            })
            .collect();
        for row in &rows {
            group.add(row);
        }
        let headers = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let has_header = std::rc::Rc::new(std::cell::Cell::new(true));
        add_mapping_controls(
            &group,
            MappingActionsParams {
                capabilities: *capabilities,
                rows: rows.clone(),
                headers: headers.clone(),
                has_header: has_header.clone(),
                columns: columns.to_vec(),
                store,
                scope,
            },
        );
        Self {
            group,
            capabilities: *capabilities,
            table: table.to_owned(),
            columns: columns.to_vec(),
            rows,
            headers,
            has_header,
            initialized: std::cell::Cell::new(false),
        }
    }

    fn refresh(&self, sheet: &CsvSheet, has_header: bool) {
        let preserve = self.initialized.get() && *self.headers.borrow() == sheet.headers;
        *self.headers.borrow_mut() = sheet.headers.clone();
        self.has_header.set(has_header);
        if preserve {
            return;
        }
        let suggestion = suggested_existing_mapping(self.capabilities, &sheet.headers, &self.columns);
        let mut labels = vec![SKIP_LABEL.to_string()];
        labels.extend(disambiguated_headers(&sheet.headers));
        for (row, field) in self.rows.iter().zip(suggestion) {
            let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
            row.set_model(Some(&gtk::StringList::new(&refs)));
            row.set_selected(field.map_or(0, |index| index as u32 + 1));
        }
        self.initialized.set(true);
    }

    fn mapping(&self) -> Vec<Option<usize>> {
        let selected = self
            .rows
            .iter()
            .map(|row| (row.selected() > 0).then(|| row.selected() as usize - 1))
            .collect();
        editable_existing_mapping(self.capabilities, &self.columns, selected)
    }
}

struct MappingActions {
    status: gtk::Label,
    capabilities: DriverCapabilities,
    rows: Vec<adw::ComboRow>,
    headers: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    has_header: std::rc::Rc<std::cell::Cell<bool>>,
    columns: Vec<ColumnInfo>,
    store: Option<CsvMappingStore>,
    scope: Option<CsvMappingScope>,
}

struct MappingActionsParams {
    capabilities: DriverCapabilities,
    rows: Vec<adw::ComboRow>,
    headers: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    has_header: std::rc::Rc<std::cell::Cell<bool>>,
    columns: Vec<ColumnInfo>,
    store: Option<CsvMappingStore>,
    scope: Option<CsvMappingScope>,
}

fn add_mapping_controls(group: &adw::PreferencesGroup, params: MappingActionsParams) {
    let actions = gtk::Box::builder().spacing(6).build();
    let remember = gtk::Button::with_label(&bookie_app_services::tr!("Remember mapping"));
    let restore = gtk::Button::with_label(&bookie_app_services::tr!("Restore mapping"));
    let forget = gtk::Button::with_label(&bookie_app_services::tr!("Forget mapping"));
    let status = gtk::Label::builder().xalign(0.0).wrap(true).build();
    let available = params.store.is_some() && params.scope.is_some();
    for button in [&remember, &restore, &forget] {
        button.set_sensitive(available);
        actions.append(button);
    }
    if !available {
        status.set_text(&bookie_app_services::tr!(
            "Saved mappings are unavailable for this connection."
        ));
    }
    group.add(&actions);
    group.add(&status);
    MappingActions::new(&status, params).connect(&remember, &restore, &forget);
}

impl MappingActions {
    fn new(status: &gtk::Label, params: MappingActionsParams) -> Self {
        Self {
            status: status.clone(),
            capabilities: params.capabilities,
            rows: params.rows,
            headers: params.headers,
            has_header: params.has_header,
            columns: params.columns,
            store: params.store,
            scope: params.scope,
        }
    }

    fn connect(&self, remember: &gtk::Button, restore: &gtk::Button, forget: &gtk::Button) {
        self.connect_remember(remember);
        self.connect_restore(restore);
        self.connect_forget(forget);
    }

    fn connect_remember(&self, button: &gtk::Button) {
        let actions = self.clone_for_signal();
        button.connect_clicked(move |_| {
            let (Some(store), Some(scope)) = (&actions.store, &actions.scope) else {
                return;
            };
            let selected = actions
                .rows
                .iter()
                .map(|row| (row.selected() > 0).then(|| row.selected() as usize - 1))
                .collect();
            let mapping = editable_existing_mapping(actions.capabilities, &actions.columns, selected);
            let result = store.save(
                scope,
                &actions.headers.borrow(),
                &actions.columns,
                &mapping,
                actions.has_header.get(),
            );
            actions.status.set_text(
                &result
                    .map(|()| bookie_app_services::tr!("Mapping saved for this table."))
                    .unwrap_or_else(|error| error),
            );
        });
    }

    fn connect_restore(&self, button: &gtk::Button) {
        let actions = self.clone_for_signal();
        button.connect_clicked(move |_| {
            let (Some(store), Some(scope)) = (&actions.store, &actions.scope) else {
                return;
            };
            match store.restore(
                scope,
                &actions.headers.borrow(),
                &actions.columns,
                actions.has_header.get(),
            ) {
                Ok(Some(mapping)) => {
                    for (row, source) in actions.rows.iter().zip(mapping) {
                        row.set_selected(source.map_or(0, |index| index as u32 + 1));
                    }
                    actions.status.set_text(&bookie_app_services::tr!(
                        "Saved mapping restored. Review it before importing."
                    ));
                }
                Ok(None) => actions
                    .status
                    .set_text(&bookie_app_services::tr!("No saved mapping for this table.")),
                Err(error) => actions.status.set_text(&error),
            }
        });
    }

    fn connect_forget(&self, button: &gtk::Button) {
        let actions = self.clone_for_signal();
        button.connect_clicked(move |_| {
            let (Some(store), Some(scope)) = (&actions.store, &actions.scope) else {
                return;
            };
            match store.forget(scope) {
                Ok(true) => actions
                    .status
                    .set_text(&bookie_app_services::tr!("Saved mapping removed.")),
                Ok(false) => actions
                    .status
                    .set_text(&bookie_app_services::tr!("No saved mapping for this table.")),
                Err(error) => actions.status.set_text(&error),
            }
        });
    }

    fn clone_for_signal(&self) -> Self {
        Self {
            status: self.status.clone(),
            capabilities: self.capabilities,
            rows: self.rows.clone(),
            headers: self.headers.clone(),
            has_header: self.has_header.clone(),
            columns: self.columns.clone(),
            store: self.store.clone(),
            scope: self.scope.clone(),
        }
    }
}

fn disambiguated_headers(headers: &[String]) -> Vec<String> {
    headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            let occurrence = headers[..=index].iter().filter(|other| *other == header).count();
            let total = headers.iter().filter(|other| *other == header).count();
            if total > 1 {
                format!("{header} [{occurrence}]")
            } else {
                header.clone()
            }
        })
        .collect()
}

fn server_managed(capabilities: DriverCapabilities, column: &ColumnInfo) -> bool {
    column.is_generated || (column.is_auto_increment && !capabilities.supports_explicit_auto_increment_values)
}

fn column_subtitle(capabilities: DriverCapabilities, column: &ColumnInfo) -> String {
    if column.is_auto_increment && capabilities.supports_explicit_auto_increment_values {
        format!(
            "{} · {}",
            column.data_type,
            bookie_app_services::tr!("Auto-increment values are inserted when mapped.")
        )
    } else if server_managed(capabilities, column) {
        format!(
            "{} · {}",
            column.data_type,
            bookie_app_services::tr!("Generated by database; CSV values are ignored.")
        )
    } else {
        column.data_type.clone()
    }
}

pub(super) fn suggested_existing_mapping(
    capabilities: DriverCapabilities,
    headers: &[String],
    columns: &[ColumnInfo],
) -> Vec<Option<usize>> {
    editable_existing_mapping(capabilities, columns, suggest_mapping(headers, columns))
}

fn editable_existing_mapping(
    capabilities: DriverCapabilities,
    columns: &[ColumnInfo],
    mapping: Vec<Option<usize>>,
) -> Vec<Option<usize>> {
    mapping
        .into_iter()
        .zip(columns)
        .map(|(field, column)| {
            if server_managed(capabilities, column) {
                None
            } else {
                field
            }
        })
        .collect()
}

struct NewColumns {
    group: adw::PreferencesGroup,
    driver_id: String,
    table: adw::EntryRow,
    type_names: Vec<&'static str>,
    rows: std::cell::RefCell<Vec<NewColumnRow>>,
}

struct NewColumnRow {
    row: adw::EntryRow,
    types: gtk::DropDown,
}

impl NewColumns {
    fn new(driver_id: &str, suggested_table: &str) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(bookie_app_services::tr!("New table"))
            .description(bookie_app_services::tr!(
                "The type of each column is a guess from the first rows of the file. Change any that is wrong before the table is created."
            ))
            .build();
        let table = adw::EntryRow::builder()
            .title(bookie_app_services::tr!("Table name"))
            .build();
        table.set_text(suggested_table);
        group.add(&table);
        Self {
            group,
            driver_id: driver_id.to_owned(),
            table,
            type_names: type_names_for(driver_id),
            rows: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn refresh(&self, sheet: &CsvSheet, options: &CsvImportOptions) {
        let mut rows = self.rows.borrow_mut();
        for existing in rows.drain(..) {
            self.group.remove(&existing.row);
        }
        for draft in infer_columns(sheet, options, &self.driver_id) {
            let row = adw::EntryRow::builder()
                .title(bookie_app_services::tr!("Column"))
                .build();
            row.set_text(&draft.name);
            let types = gtk::DropDown::from_strings(&self.type_names);
            types.set_valign(gtk::Align::Center);
            types.set_selected(self.position_of(&draft.data_type));
            row.add_suffix(&types);
            self.group.add(&row);
            rows.push(NewColumnRow { row, types });
        }
    }

    fn position_of(&self, data_type: &str) -> u32 {
        self.type_names.iter().position(|name| *name == data_type).unwrap_or(0) as u32
    }

    fn resolve(&self) -> Option<ResolvedTarget> {
        let table = self.table.text().trim().to_owned();
        let table_ok = sql_dialect::validate_ident(&table).is_ok();
        mark(&self.table, table_ok);
        let rows = self.rows.borrow();
        let mut drafts = Vec::with_capacity(rows.len());
        let mut all_ok = table_ok && !rows.is_empty();
        for entry in rows.iter() {
            let name = entry.row.text().trim().to_owned();
            let ok = sql_dialect::validate_ident(&name).is_ok();
            mark(&entry.row, ok);
            all_ok = all_ok && ok;
            drafts.push(DraftColumn {
                original: None,
                name,
                data_type: self.selected_type(entry).to_owned(),
                nullable: true,
                primary_key: false,
                auto_increment: false,
                default_value: None,
                comment: None,
                collation: None,
            });
        }
        if !all_ok {
            return None;
        }
        let columns: Vec<ColumnInfo> = drafts.iter().map(draft_to_info).collect();
        let mapping = (0..columns.len()).map(Some).collect();
        Some((table, columns, mapping, Some(drafts)))
    }

    fn selected_type(&self, entry: &NewColumnRow) -> &'static str {
        self.type_names
            .get(entry.types.selected() as usize)
            .copied()
            .unwrap_or("TEXT")
    }
}

/// A column the import is about to create: what the table will hold once
/// the CREATE has run, which is what the INSERT binds against.
fn draft_to_info(draft: &DraftColumn) -> ColumnInfo {
    ColumnInfo {
        name: draft.name.clone(),
        data_type: draft.data_type.clone(),
        nullable: draft.nullable,
        primary_key: draft.primary_key,
        is_auto_increment: draft.auto_increment,
        default_value: draft.default_value.clone(),
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn mark(row: &adw::EntryRow, ok: bool) {
    if ok {
        row.remove_css_class("error");
        return;
    }
    row.add_css_class("error");
}

fn type_names_for(driver_id: &str) -> Vec<&'static str> {
    let mut names = Vec::new();
    for kind in bookie_core::import::ColumnKind::ALL {
        let name = bookie_core::import::type_name(driver_id, kind);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

fn starting_encoding(bytes: &[u8]) -> u32 {
    let encoding = if bytes.starts_with(&[0xff, 0xfe]) {
        CsvImportEncoding::Utf16Le
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        CsvImportEncoding::Utf16Be
    } else if std::str::from_utf8(bytes).is_err() {
        CsvImportEncoding::Windows1252
    } else {
        CsvImportEncoding::Utf8
    };
    CsvImportEncoding::ALL
        .iter()
        .position(|candidate| *candidate == encoding)
        .unwrap_or(0) as u32
}

fn choice_row(title: &str, labels: &[&str]) -> adw::ComboRow {
    adw::ComboRow::builder()
        .use_markup(false)
        .title(title)
        .model(&gtk::StringList::new(labels))
        .build()
}

fn delimiter_row(selected: CsvDelimiter) -> adw::ComboRow {
    let labels: Vec<&str> = CsvDelimiter::ALL
        .iter()
        .map(|delimiter| delimiter_label(*delimiter))
        .collect();
    let row = adw::ComboRow::builder()
        .title(bookie_app_services::tr!("Field separator"))
        .model(&gtk::StringList::new(&labels))
        .build();
    row.set_selected(
        CsvDelimiter::ALL
            .iter()
            .position(|delimiter| *delimiter == selected)
            .unwrap_or(0) as u32,
    );
    row
}

fn delimiter_label(delimiter: CsvDelimiter) -> &'static str {
    match delimiter {
        CsvDelimiter::Comma => "Comma",
        CsvDelimiter::Semicolon => "Semicolon",
        CsvDelimiter::Tab => "Tab",
        CsvDelimiter::Pipe => "Pipe",
    }
}

fn selected_delimiter(index: u32) -> CsvDelimiter {
    CsvDelimiter::ALL
        .get(index as usize)
        .copied()
        .unwrap_or(CsvDelimiter::Comma)
}

fn build_dialog(page: &adw::PreferencesPage, start_button: &gtk::Button) -> adw::Dialog {
    let footer = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .halign(gtk::Align::End)
        .margin_top(6)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    footer.append(start_button);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(page));
    toolbar.add_bottom_bar(&footer);
    let dialog = adw::Dialog::builder()
        .title(bookie_app_services::tr!("Import CSV"))
        .content_width(560)
        .content_height(680)
        .child(&toolbar)
        .build();
    dialog.set_default_widget(Some(start_button));
    dialog
}

pub(crate) struct ImportProgress {
    window: CancellableDialog,
    bar: gtk::ProgressBar,
    label: gtk::Label,
    spinner: gtk::Spinner,
    total: Option<u64>,
    file_note: Option<String>,
}

impl ImportProgress {
    pub(crate) fn cancel_token(&self) -> CancellationToken {
        self.window.cancel_token()
    }

    pub(crate) fn set_committed(&self, committed: u64) {
        let total = self.total.unwrap_or_default();
        let fraction = if total == 0 {
            0.0
        } else {
            committed as f64 / total as f64
        };
        self.bar.set_fraction(fraction);
        self.label.set_label(&self.progress_text(committed, total));
    }

    pub(crate) fn begin_file(&mut self, note: &str, total: u64) {
        self.file_note = Some(note.to_owned());
        self.set_total(total);
    }

    fn progress_text(&self, committed: u64, total: u64) -> String {
        match &self.file_note {
            Some(note) => format!("{note}\n{}", rows_label(committed, total)),
            None => rows_label(committed, total),
        }
    }

    pub(crate) fn set_total(&mut self, total: u64) {
        self.total = Some(total);
        self.spinner.stop();
        self.spinner.set_visible(false);
        self.bar.set_visible(true);
        self.bar.set_fraction(0.0);
        self.label.set_label(&self.progress_text(0, total));
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.window.cancel_token().is_cancelled()
    }

    pub(crate) fn close(&self) {
        self.window.close();
    }
}

fn rows_label(committed: u64, total: u64) -> String {
    bookie_app_services::tr!("{done} of {total} rows")
        .replace("{done}", &committed.to_string())
        .replace("{total}", &total.to_string())
}

pub(crate) fn present_preparing(parent: &adw::ApplicationWindow, table: &str) -> ImportProgress {
    let label = gtk::Label::new(Some(&bookie_app_services::tr!("Reading and planning the import…")));
    let spinner = gtk::Spinner::new();
    spinner.start();
    let bar = gtk::ProgressBar::new();
    bar.set_visible(false);
    let title = bookie_app_services::tr!("Preparing import into {table}").replace("{table}", table);
    let window = CancellableDialog::present(
        parent,
        &title,
        &[label.upcast_ref(), spinner.upcast_ref(), bar.upcast_ref()],
    );
    ImportProgress {
        window,
        bar,
        label,
        spinner,
        total: None,
        file_note: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_is_not_utf8_opens_with_a_matching_encoding() {
        let selected = |bytes: &[u8]| CsvImportEncoding::ALL[starting_encoding(bytes) as usize];
        assert_eq!(selected(b"a,b\n1,2\n"), CsvImportEncoding::Utf8);
        assert_eq!(selected(b"caf\xe9\n"), CsvImportEncoding::Windows1252);
        assert_eq!(selected(&[0xff, 0xfe, b'a', 0]), CsvImportEncoding::Utf16Le);
        assert_eq!(selected(&[0xfe, 0xff, 0, b'a']), CsvImportEncoding::Utf16Be);
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn finishing_an_import_force_closes_the_non_user_closable_progress_dialog() {
        gtk::init().unwrap();
        adw::init().unwrap();
        gtk::Settings::default().unwrap().set_gtk_enable_animations(false);
        let application = gtk::Application::new(None::<&str>, gtk::gio::ApplicationFlags::NON_UNIQUE);
        application.register(None::<&gtk::gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&application);
        window.present();
        let mut progress = present_preparing(&window, "items");

        assert!(wait_until(|| progress.window.dialog().is_mapped()));
        progress.set_total(2);
        let closed = std::rc::Rc::new(std::cell::Cell::new(false));
        let closed_for_signal = closed.clone();
        progress
            .window
            .dialog()
            .connect_closed(move |_| closed_for_signal.set(true));
        progress.close();
        assert!(wait_until(|| closed.get()));
        window.close();
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn csv_preparation_progress_changes_from_spinner_to_row_count() {
        gtk::init().unwrap();
        adw::init().unwrap();
        let application = gtk::Application::new(None::<&str>, gtk::gio::ApplicationFlags::NON_UNIQUE);
        application.register(None::<&gtk::gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&application);
        window.present();
        let mut progress = present_preparing(&window, "people");

        assert!(wait_until(|| progress.window.dialog().is_mapped()));
        assert!(progress.spinner.is_spinning());
        assert!(progress.spinner.is_visible());
        assert!(!progress.bar.is_visible());
        assert!(progress.label.label().contains("planning"));
        progress.set_total(42);
        assert!(!progress.spinner.is_spinning());
        assert!(!progress.spinner.is_visible());
        assert!(progress.bar.is_visible());
        assert!(progress.label.label().contains("42"));
        progress.close();
        window.close();
    }

    fn wait_until(predicate: impl Fn() -> bool) -> bool {
        let context = gtk::glib::MainContext::default();
        for _ in 0..200 {
            for _ in 0..64 {
                if !context.iteration(false) {
                    break;
                }
            }
            if predicate() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        predicate()
    }

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "INTEGER".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }
    }

    #[test]
    fn preview_suggestions_keep_explicit_auto_increment_and_skip_generated_columns() {
        let headers = vec!["id".into(), "name".into(), "computed".into()];
        let columns = vec![
            ColumnInfo {
                is_auto_increment: true,
                ..column("id")
            },
            ColumnInfo {
                data_type: "TEXT".into(),
                ..column("name")
            },
            ColumnInfo {
                is_generated: true,
                ..column("computed")
            },
        ];

        assert_eq!(
            suggested_existing_mapping(explicit_auto_increment_capabilities(), &headers, &columns),
            [Some(0), Some(1), None]
        );
        assert_eq!(
            suggested_existing_mapping(DriverCapabilities::default(), &headers, &columns),
            [None, Some(1), None]
        );
    }

    #[test]
    fn selected_preview_mapping_keeps_explicit_auto_increment_and_skips_generated_columns() {
        let columns = vec![
            ColumnInfo {
                is_auto_increment: true,
                ..column("id")
            },
            column("name"),
            ColumnInfo {
                is_generated: true,
                ..column("computed")
            },
        ];

        assert_eq!(
            editable_existing_mapping(
                explicit_auto_increment_capabilities(),
                &columns,
                vec![Some(0), Some(1), Some(2)]
            ),
            [Some(0), Some(1), None]
        );
        assert_eq!(
            editable_existing_mapping(DriverCapabilities::default(), &columns, vec![Some(0), Some(1), Some(2)]),
            [None, Some(1), None]
        );
        assert!(!server_managed(explicit_auto_increment_capabilities(), &columns[0]));
        assert!(server_managed(DriverCapabilities::default(), &columns[0]));
        assert!(server_managed(explicit_auto_increment_capabilities(), &columns[2]));
    }

    fn explicit_auto_increment_capabilities() -> DriverCapabilities {
        DriverCapabilities {
            supports_explicit_auto_increment_values: true,
            ..DriverCapabilities::default()
        }
    }

    #[test]
    fn duplicate_header_labels_are_distinguishable_without_changing_header_values() {
        let headers = vec!["tag".to_string(), "tag".to_string(), "email".to_string()];
        assert_eq!(disambiguated_headers(&headers), ["tag [1]", "tag [2]", "email"]);
        assert_eq!(headers, ["tag", "tag", "email"]);
    }
}

#[cfg(test)]
mod lifetime_tests;
