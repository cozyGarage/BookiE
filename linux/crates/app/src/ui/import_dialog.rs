use std::path::PathBuf;

use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use tablepro_core::ColumnInfo;
use tablepro_core::export::CsvDelimiter;
use tablepro_core::import::{CsvFormat, CsvImportOptions, CsvSheet, MAX_PREVIEW_ROWS, read_csv, suggest_mapping};
use tokio_util::sync::CancellationToken;

/// What the app read from the file before it asked the user anything.
pub(crate) struct ImportRequest {
    pub schema: Option<String>,
    pub table: String,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub format: CsvFormat,
    pub columns: Vec<ColumnInfo>,
}

/// What the user settled on. The app re-reads the whole file with these
/// options; the dialog only ever read a preview.
#[derive(Debug, Clone)]
pub(crate) struct ImportChoice {
    pub schema: Option<String>,
    pub table: String,
    pub bytes: Vec<u8>,
    pub options: CsvImportOptions,
    pub mapping: Vec<Option<usize>>,
    pub columns: Vec<ColumnInfo>,
}

const SKIP_LABEL: &str = "Skip";

pub(crate) fn present(
    parent: &adw::ApplicationWindow,
    request: ImportRequest,
    on_start: impl Fn(ImportChoice) + 'static,
) {
    let page = adw::PreferencesPage::new();
    let file_group = adw::PreferencesGroup::builder()
        .title(crate::tr!("File"))
        .description(request.path.display().to_string())
        .build();
    let delimiter_row = delimiter_row(request.format.delimiter);
    let header_row = adw::SwitchRow::builder()
        .title(crate::tr!("First row names the columns"))
        .active(request.format.has_header)
        .build();
    let null_row = adw::EntryRow::builder()
        .title(crate::tr!("Text that means NULL"))
        .build();
    file_group.add(&delimiter_row);
    file_group.add(&header_row);
    file_group.add(&null_row);
    page.add(&file_group);

    let mapping_group = adw::PreferencesGroup::builder()
        .title(crate::tr!("Columns"))
        .description(crate::tr!(
            "A column left on Skip is not written, so the database applies its own default."
        ))
        .build();
    page.add(&mapping_group);

    let rows: Vec<adw::ComboRow> = request.columns.iter().map(column_row).collect();
    for row in &rows {
        mapping_group.add(row);
    }

    let state = std::rc::Rc::new(State {
        request,
        rows,
        delimiter_row,
        header_row,
        null_row,
    });
    state.refresh();
    connect_refresh(&state);

    let start_button = gtk::Button::builder().label(crate::tr!("Import")).build();
    start_button.add_css_class("suggested-action");
    let dialog = build_dialog(&page, &start_button);
    let dialog_for_start = dialog.clone();
    start_button.connect_clicked(move |_| {
        dialog_for_start.close();
        on_start(state.choice());
    });
    dialog.present(Some(parent));
}

struct State {
    request: ImportRequest,
    rows: Vec<adw::ComboRow>,
    delimiter_row: adw::ComboRow,
    header_row: adw::SwitchRow,
    null_row: adw::EntryRow,
}

impl State {
    fn options(&self) -> CsvImportOptions {
        CsvImportOptions {
            delimiter: selected_delimiter(self.delimiter_row.selected()),
            has_header: self.header_row.is_active(),
            null_marker: self.null_row.text().to_string(),
        }
    }

    fn preview(&self) -> CsvSheet {
        read_csv(&self.request.bytes, &self.options(), Some(MAX_PREVIEW_ROWS)).unwrap_or_default()
    }

    /// Re-read the preview with the options as they now stand and offer
    /// every field name the file turned out to have.
    fn refresh(&self) {
        let sheet = self.preview();
        let suggestion = suggest_mapping(&sheet.headers, &self.request.columns);
        let mut labels: Vec<&str> = vec![SKIP_LABEL];
        labels.extend(sheet.headers.iter().map(String::as_str));
        for (row, field) in self.rows.iter().zip(suggestion) {
            row.set_model(Some(&gtk::StringList::new(&labels)));
            row.set_selected(field.map_or(0, |index| index as u32 + 1));
        }
    }

    fn choice(&self) -> ImportChoice {
        ImportChoice {
            schema: self.request.schema.clone(),
            table: self.request.table.clone(),
            bytes: self.request.bytes.clone(),
            options: self.options(),
            mapping: self
                .rows
                .iter()
                .map(|row| (row.selected() > 0).then(|| row.selected() as usize - 1))
                .collect(),
            columns: self.request.columns.clone(),
        }
    }
}

fn connect_refresh(state: &std::rc::Rc<State>) {
    let for_delimiter = state.clone();
    state
        .delimiter_row
        .connect_selected_notify(move |_| for_delimiter.refresh());
    let for_header = state.clone();
    state.header_row.connect_active_notify(move |_| for_header.refresh());
}

fn column_row(column: &ColumnInfo) -> adw::ComboRow {
    adw::ComboRow::builder()
        .title(&column.name)
        .subtitle(&column.data_type)
        .build()
}

fn delimiter_row(selected: CsvDelimiter) -> adw::ComboRow {
    let labels: Vec<&str> = CsvDelimiter::ALL
        .iter()
        .map(|delimiter| delimiter_label(*delimiter))
        .collect();
    let row = adw::ComboRow::builder()
        .title(crate::tr!("Field separator"))
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
        .title(crate::tr!("Import CSV"))
        .content_width(520)
        .content_height(640)
        .child(&toolbar)
        .build();
    dialog.set_default_widget(Some(start_button));
    dialog
}

/// The running import's progress window. The app holds it while the
/// batches run and drops it when the import reaches its terminal state.
pub(crate) struct ImportProgress {
    dialog: adw::Dialog,
    bar: gtk::ProgressBar,
    label: gtk::Label,
    total: u64,
    cancel: CancellationToken,
}

impl ImportProgress {
    pub(crate) fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub(crate) fn set_committed(&self, committed: u64) {
        let fraction = if self.total == 0 {
            0.0
        } else {
            committed as f64 / self.total as f64
        };
        self.bar.set_fraction(fraction);
        self.label.set_label(
            &crate::tr!("{done} of {total} rows")
                .replace("{done}", &committed.to_string())
                .replace("{total}", &self.total.to_string()),
        );
    }

    pub(crate) fn close(&self) {
        self.dialog.close();
    }
}

pub(crate) fn present_progress(parent: &adw::ApplicationWindow, table: &str, total: u64) -> ImportProgress {
    let cancel = CancellationToken::new();
    let label = gtk::Label::new(Some(
        &crate::tr!("{done} of {total} rows")
            .replace("{done}", "0")
            .replace("{total}", &total.to_string()),
    ));
    let bar = gtk::ProgressBar::new();
    let stop = gtk::Button::with_label(&crate::tr!("Cancel"));
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    content.append(&label);
    content.append(&bar);
    content.append(&stop);
    let dialog = adw::Dialog::builder()
        .title(crate::tr!("Importing into {table}").replace("{table}", table))
        .content_width(400)
        .child(&content)
        .can_close(false)
        .build();
    stop.connect_clicked({
        let cancel = cancel.clone();
        move |button| {
            cancel.cancel();
            button.set_sensitive(false);
        }
    });
    dialog.present(Some(parent));
    ImportProgress {
        dialog,
        bar,
        label,
        total,
        cancel,
    }
}
