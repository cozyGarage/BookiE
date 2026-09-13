use relm4::adw::prelude::*;
use relm4::gtk::gio;
use relm4::{adw, gtk};
use tablepro_core::QueryResult;

use crate::services::preferences;

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
    result: QueryResult,
    suggested_name: String,
) {
    let page = adw::PreferencesPage::new();

    let format_group = adw::PreferencesGroup::new();
    let labels: Vec<&str> = ExportFormat::ALL.iter().map(|format| format.label()).collect();
    let format_row = adw::ComboRow::builder()
        .title(crate::tr!("Format"))
        .subtitle(crate::tr!("{n} rows").replace("{n}", &result.rows.len().to_string()))
        .model(&gtk::StringList::new(&labels))
        .build();
    format_group.add(&format_row);
    page.add(&format_group);

    let csv_group = adw::PreferencesGroup::builder()
        .title(crate::tr!("CSV options"))
        .build();
    let include_header = adw::SwitchRow::builder()
        .title(crate::tr!("Include column names"))
        .subtitle(crate::tr!("Write column names in the first row"))
        .build();
    include_header.set_active(preferences::load().csv_include_header);
    csv_group.add(&include_header);
    page.add(&csv_group);

    include_header.connect_active_notify(move |row| {
        preferences::update(|prefs| prefs.csv_include_header = row.is_active());
    });

    let csv_group_for_format = csv_group.clone();
    format_row.connect_selected_notify(move |row| {
        csv_group_for_format.set_visible(selected_format(row.selected()) == ExportFormat::Csv);
    });

    let reset_button = gtk::Button::builder().label(crate::tr!("Reset to Defaults")).build();
    reset_button.add_css_class("flat");
    let include_header_for_reset = include_header.clone();
    reset_button.connect_clicked(move |_| include_header_for_reset.set_active(true));

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
            format,
            &suggested_name,
            result.clone(),
            include_header,
        );
    });

    dialog.present(Some(parent));
}

fn save_with_file_dialog(
    parent: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
    format: ExportFormat,
    suggested_name: &str,
    result: QueryResult,
    include_header: bool,
) {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&crate::tr!("{format} files").replace("{format}", format.label())));
    filter.add_mime_type(format.mime_type());
    filter.add_suffix(format.extension());
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let file_dialog = gtk::FileDialog::builder()
        .title(crate::tr!("Export Results"))
        .modal(true)
        .initial_name(suggested_file_name(suggested_name, format))
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
        let write_result = match format {
            ExportFormat::Csv => write_csv(&path, &result, include_header),
            ExportFormat::Json => write_json(&path, &result),
        };
        match write_result {
            Ok(()) => toast_overlay.add_toast(adw::Toast::new(
                &crate::tr!("Exported {n} rows to {path}")
                    .replace("{n}", &result.rows.len().to_string())
                    .replace("{path}", &path.display().to_string()),
            )),
            Err(error) => show_export_error(&parent_for_alert, &path, &error),
        }
    });
}

fn write_csv(path: &std::path::Path, result: &QueryResult, include_header: bool) -> std::io::Result<()> {
    tablepro_core::export::write_atomically(path, |mut output| {
        if include_header {
            tablepro_core::export::write_csv_header(&mut output, &result.columns)?;
        }
        for row in &result.rows {
            tablepro_core::export::write_csv_row(&mut output, row)?;
        }
        Ok(())
    })
}

fn write_json(path: &std::path::Path, result: &QueryResult) -> std::io::Result<()> {
    let json = tablepro_core::export::render_json(&result.columns, &result.rows);
    tablepro_core::export::write_atomically(path, |output| {
        std::io::Write::write_all(output, json.as_bytes())?;
        std::io::Write::write_all(output, b"\n")
    })
}

fn show_export_error(parent: &adw::ApplicationWindow, path: &std::path::Path, error: &std::io::Error) {
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

    #[test]
    fn json_export_keeps_duplicate_columns_and_binary_values() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("result.json");
        let result = QueryResult {
            columns: vec![column("id"), column("id")],
            rows: vec![vec![
                tablepro_core::Value::Bytes(vec![0, 0xff]),
                tablepro_core::Value::Int(2),
            ]],
            truncated: false,
        };

        write_json(&path, &result).unwrap();

        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "[\n  {\n    \"id\": \"0x00ff\",\n    \"id_2\": 2\n  }\n]\n"
        );
    }

    fn column(name: &str) -> tablepro_core::ColumnInfo {
        tablepro_core::ColumnInfo {
            name: name.to_string(),
            data_type: "text".to_string(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
        }
    }
}
