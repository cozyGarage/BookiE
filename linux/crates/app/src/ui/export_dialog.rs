mod file;

use relm4::adw::prelude::*;
use relm4::gtk::gio;
use relm4::{adw, gtk};
use tablepro_core::QueryResult;

use crate::services::preferences::PreferencesStore;

pub(crate) struct ExportRequest {
    pub(crate) result: QueryResult,
    pub(crate) suggested_name: String,
    pub(crate) driver_id: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExportFormat {
    Csv,
    Json,
}

impl ExportFormat {
    const ALL: [Self; 2] = [Self::Csv, Self::Json];

    fn label(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Json => "JSON",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
        }
    }

    fn mime_type(self) -> &'static str {
        match self {
            Self::Csv => "text/csv",
            Self::Json => "application/json",
        }
    }
}

fn selected_format(index: u32) -> ExportFormat {
    ExportFormat::ALL
        .get(index as usize)
        .copied()
        .unwrap_or(ExportFormat::Csv)
}

fn insert_target(name: &str) -> (Option<String>, String) {
    match name.rsplit_once('.') {
        Some((schema, table)) if !schema.is_empty() && !table.is_empty() => {
            (Some(schema.to_string()), table.to_string())
        }
        _ => (None, name.to_string()),
    }
}

fn suggested_file_name(name: &str, format: ExportFormat) -> String {
    let base = name
        .strip_suffix(".csv")
        .or_else(|| name.strip_suffix(".json"))
        .filter(|base| !base.is_empty())
        .unwrap_or(name);
    format!("{base}.{}", format.extension())
}

pub(crate) fn present(
    parent: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
    request: ExportRequest,
    preferences: &PreferencesStore,
) {
    present_with_format(parent, toast_overlay, request, false, preferences);
}

pub(crate) fn present_with_format(
    parent: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
    request: ExportRequest,
    json: bool,
    preferences: &PreferencesStore,
) {
    let ExportRequest {
        result,
        suggested_name,
        driver_id,
    } = request;
    let page = adw::PreferencesPage::new();

    let format_group = adw::PreferencesGroup::new();
    let labels: Vec<&str> = ExportFormat::ALL.iter().map(|format| format.label()).collect();
    let format_row = adw::ComboRow::builder()
        .title(crate::tr!("Format"))
        .subtitle(
            if result.truncated {
                crate::tr!("{n} loaded rows (result truncated)")
            } else {
                crate::tr!("{n} loaded rows / current page")
            }
            .replace("{n}", &result.rows.len().to_string()),
        )
        .model(&gtk::StringList::new(&labels))
        .build();
    format_row.set_selected(u32::from(json));
    format_group.add(&format_row);
    page.add(&format_group);

    let csv_group = adw::PreferencesGroup::builder()
        .title(crate::tr!("CSV options"))
        .build();
    let include_header = adw::SwitchRow::builder()
        .title(crate::tr!("Include column names"))
        .subtitle(crate::tr!("Write column names in the first row"))
        .build();
    include_header.set_active(preferences.load().csv_include_header);
    csv_group.add(&include_header);
    let safe_csv = adw::SwitchRow::builder()
        .title(crate::tr!("Spreadsheet-safe text"))
        .subtitle(crate::tr!(
            "Prevent text and column names from being interpreted as formulas. Turn off for raw text."
        ))
        .active(true)
        .build();
    csv_group.add(&safe_csv);
    csv_group.set_visible(!json);
    page.add(&csv_group);

    let preferences_for_toggle = preferences.clone();
    include_header.connect_active_notify(move |row| {
        preferences_for_toggle.update(|prefs| prefs.csv_include_header = row.is_active());
    });

    let csv_group_for_format = csv_group.clone();
    format_row.connect_selected_notify(move |row| {
        csv_group_for_format.set_visible(selected_format(row.selected()) == ExportFormat::Csv);
    });

    let reset_button = gtk::Button::builder().label(crate::tr!("Reset to Defaults")).build();
    reset_button.add_css_class("flat");
    let include_header_for_reset = include_header.clone();
    let safe_csv_for_reset = safe_csv.clone();
    reset_button.connect_clicked(move |_| {
        include_header_for_reset.set_active(true);
        safe_csv_for_reset.set_active(true);
    });

    let export_button = gtk::Button::builder().label(crate::tr!("Export\u{2026}")).build();
    export_button.add_css_class("suggested-action");

    let footer = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .margin_top(6)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    footer.append(&reset_button);
    footer.append(&gtk::Box::builder().hexpand(true).build());
    footer.append(&export_button);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&page));
    toolbar.add_bottom_bar(&footer);

    let dialog = adw::Dialog::builder()
        .title(crate::tr!("Export Results"))
        .content_width(480)
        .child(&toolbar)
        .build();
    dialog.set_default_widget(Some(&export_button));

    let parent_for_export = parent.clone();
    let toast_overlay_for_export = toast_overlay.clone();
    let dialog_for_export = dialog.clone();
    let include_header_for_export = include_header.clone();
    export_button.connect_clicked(move |_| {
        let format = selected_format(format_row.selected());
        let include_header = include_header_for_export.is_active();
        dialog_for_export.close();
        save_with_file_dialog(
            &parent_for_export,
            &toast_overlay_for_export,
            SaveRequest {
                format,
                suggested_name: suggested_name.clone(),
                driver_id: driver_id.clone(),
                result: result.clone(),
                include_header,
                sanitize_formulas: safe_csv.is_active(),
            },
        );
    });

    dialog.present(Some(parent));
}

struct SaveRequest {
    format: ExportFormat,
    suggested_name: String,
    driver_id: String,
    result: QueryResult,
    include_header: bool,
    sanitize_formulas: bool,
}

fn save_with_file_dialog(parent: &adw::ApplicationWindow, toast_overlay: &adw::ToastOverlay, request: SaveRequest) {
    let SaveRequest {
        format,
        suggested_name,
        driver_id,
        result,
        include_header,
        sanitize_formulas,
    } = request;
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&crate::tr!("{format} files").replace("{format}", format.label())));
    filter.add_mime_type(format.mime_type());
    filter.add_suffix(format.extension());
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let file_dialog = gtk::FileDialog::builder()
        .title(crate::tr!("Export Results"))
        .modal(true)
        .initial_name(suggested_file_name(&suggested_name, format))
        .default_filter(&filter)
        .filters(&filters)
        .build();

    let parent_for_alert = parent.clone();
    let toast_overlay = toast_overlay.clone();
    file_dialog.save(Some(parent), gio::Cancellable::NONE, move |outcome| {
        let Ok(file) = outcome else {
            return;
        };
        let Some(path) = file.path() else {
            return;
        };
        let options = tablepro_core::export::CsvOptions {
            header_row: include_header,
            sanitize_formulas,
            ..Default::default()
        };
        let (schema, table) = insert_target(&suggested_name);
        file::start(
            &parent_for_alert,
            &toast_overlay,
            file::ExportJob {
                path,
                result: result.clone(),
                format,
                options,
                driver_id: driver_id.clone(),
                schema,
                table,
            },
        );
    });
}

fn show_export_error(
    parent: &adw::ApplicationWindow,
    path: &std::path::Path,
    error: &tablepro_core::export::ExportError,
) {
    let alert = adw::AlertDialog::new(
        Some(&crate::tr!("Couldn't export")),
        Some(
            &crate::tr!("Writing {path} failed: {error}")
                .replace("{path}", &path.display().to_string())
                .replace("{error}", &error.to_string()),
        ),
    );
    alert.add_response("close", &crate::tr!("Close"));
    alert.set_default_response(Some("close"));
    alert.set_close_response("close");
    alert.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_qualified_result_name_becomes_a_schema_and_table_for_sql_statements() {
        assert_eq!(
            insert_target("public.customers"),
            (Some("public".into()), "customers".into())
        );
        assert_eq!(insert_target("customers"), (None, "customers".into()));
        assert_eq!(insert_target("Query 1"), (None, "Query 1".into()));
        assert_eq!(insert_target(".customers"), (None, ".customers".into()));
    }

    #[test]
    fn suggested_file_name_uses_the_selected_format() {
        assert_eq!(suggested_file_name("customers", ExportFormat::Csv), "customers.csv");
        assert_eq!(
            suggested_file_name("customers.csv", ExportFormat::Json),
            "customers.json"
        );
        assert_eq!(
            suggested_file_name("customers.json", ExportFormat::Csv),
            "customers.csv"
        );
    }
}
