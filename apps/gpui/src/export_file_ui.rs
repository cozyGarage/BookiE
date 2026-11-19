use std::path::PathBuf;

use bookie_core::export::{ResultExport, ResultFormat, SqlTarget, write_result_file};
use bookie_core::{QueryResult, QueryRows};

use super::*;
use crate::chrome::ghost;

#[derive(Default)]
pub(crate) struct ExportFileState {
    pub open: bool,
    pub input: Option<Entity<TextInput>>,
}

pub(crate) fn format_for_extension(path: &std::path::Path) -> Option<ResultFormat> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "csv" => ResultFormat::Csv,
        "json" => ResultFormat::Json,
        "jsonl" | "ndjson" => ResultFormat::JsonLines,
        "md" | "markdown" => ResultFormat::Markdown,
        "html" | "htm" => ResultFormat::Html,
        "xml" => ResultFormat::Xml,
        "sql" => ResultFormat::Sql,
        "xlsx" => ResultFormat::Xlsx,
        _ => return None,
    })
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

impl GridView {
    pub(crate) fn export_file_enabled(&self) -> bool {
        !self.export.file.open && self.dataset_count() > 0
    }

    pub(crate) fn close_export_file_bar(&mut self) {
        self.export.file.open = false;
    }

    pub(crate) fn open_export_file_bar(&mut self, cx: &mut Context<Self>) {
        if !self.export_file_enabled() {
            return;
        }
        let input = self.export.file.input.get_or_insert_with(|| {
            let input =
                cx.new(|cx| TextInput::new("Path ending in .csv .json .jsonl .md .html .xml .sql or .xlsx", cx));
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submit) {
                    this.write_export_file(cx);
                }
            })
            .detach();
            input
        });
        input.update(cx, |input, cx| input.set_content("", cx));
        self.export.file.open = true;
        self.status = "Type where to write the current page or selection".into();
        cx.notify();
    }

    fn export_destination(&mut self, typed: &str) -> Option<(PathBuf, ResultFormat, Option<bookie_core::TableInfo>)> {
        let path = expand_home(typed);
        let Some(format) = format_for_extension(&path) else {
            self.status = "Use a file name ending in .csv, .json, .jsonl, .md, .html, .xml, .sql or .xlsx".into();
            return None;
        };
        if typed.is_empty() || path.exists() {
            self.status = "That file already exists or no path was given; choose a new file name".into();
            return None;
        }
        let target = (!self.grid_is_query).then(|| self.catalog.grid_table.clone());
        if format == ResultFormat::Sql && target.is_none() {
            self.status = "SQL export needs a table; open a table, not a query result".into();
            return None;
        }
        Some((path, format, target))
    }

    pub(crate) fn write_export_file(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.export.file.input.clone().filter(|_| self.export.file.open) else {
            return;
        };
        let typed = input.read(cx).content().trim().to_string();
        let Some((path, format, target)) = self.export_destination(&typed) else {
            return;
        };
        let Some((rows, count, cut)) = self.rows_for_export() else {
            return;
        };
        let driver_id = self
            .live
            .as_ref()
            .map_or_else(|| "postgres".to_string(), |session| session.driver_id.clone());
        let result = QueryResult {
            columns: column_infos_for(self.source.columns()),
            rows: QueryRows::from_rows(rows),
            truncated: false,
        };
        let csv = self.csv_options();
        self.status = format!("Writing {count} row(s)…");
        let shown = path.display().to_string();
        cx.spawn(async move |this, cx| {
            let written = cx
                .background_executor()
                .spawn(async move {
                    let sql = target.as_ref().map(|table| SqlTarget {
                        driver_id: &driver_id,
                        schema: table.schema.as_deref(),
                        table: &table.name,
                    });
                    let export = ResultExport { format, csv: &csv, sql };
                    write_result_file(&path, &result, &export, || false, |_| {}).map_err(|error| error.to_string())
                })
                .await;
            this.update(cx, |this, cx| {
                match written {
                    Ok(()) => {
                        this.close_export_file_bar();
                        this.status = if cut {
                            format!("Wrote the first {count} row(s) to {shown} (export limit reached)")
                        } else {
                            format!("Wrote {count} row(s) to {shown}")
                        };
                    }
                    Err(message) => this.status = format!("Could not export: {message}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn export_file_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let input = self.export.file.input.clone()?;
        if !self.export.file.open {
            return None;
        }
        Some(
            div()
                .flex()
                .items_center()
                .gap_3()
                .p_2()
                .flex_none()
                .bg(rgb(theme.chrome))
                .border_b_1()
                .border_color(rgb(theme.border))
                .child(div().font_weight(gpui::FontWeight::MEDIUM).child("Export to file"))
                .child(div().flex_1().min_w_0().child(input))
                .child(
                    ghost("write-export-file", "Export", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.write_export_file(cx))),
                )
                .child(
                    ghost("cancel-export-file", "Cancel", theme).on_click(cx.listener(|this, _, _, cx| {
                        this.close_export_file_bar();
                        cx.notify();
                    })),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_picks_the_format_case_insensitively_and_unknown_ones_are_refused() {
        let pick = |name: &str| format_for_extension(std::path::Path::new(name));
        assert_eq!(pick("a.CSV"), Some(ResultFormat::Csv));
        assert_eq!(pick("a.ndjson"), Some(ResultFormat::JsonLines));
        assert_eq!(pick("a.md"), Some(ResultFormat::Markdown));
        assert_eq!(pick("a.xlsx"), Some(ResultFormat::Xlsx));
        assert_eq!(pick("a.txt"), None);
        assert_eq!(pick("noextension"), None);
    }
}
