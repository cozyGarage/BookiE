use std::path::PathBuf;

use bookie_app_services::export_tables::{TableOutcome, TableResult};
use bookie_gpui::messages::MessageKind;

use super::*;
use crate::backup_ui::absolute_path;
use crate::chrome::ghost;

pub(crate) fn export_folder(typed: &str) -> Result<PathBuf, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err("Type the folder to write the CSV files into".into());
    }
    let path = absolute_path(typed)?;
    if path.is_dir() {
        Ok(path)
    } else {
        Err("That folder does not exist".into())
    }
}

pub(crate) fn row_limit(typed: &str) -> Result<Option<usize>, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Ok(None);
    }
    match typed.parse::<usize>() {
        Ok(limit) if limit > 0 => Ok(Some(limit)),
        _ => Err("The row limit must be a whole number above zero, or empty for no limit".into()),
    }
}

pub(crate) fn export_report(results: &[TableResult]) -> Vec<(MessageKind, String)> {
    results
        .iter()
        .map(|result| match &result.outcome {
            TableOutcome::Written { rows, capped } => (
                MessageKind::Success,
                format!(
                    "{}: {rows} rows{}",
                    result.table,
                    if *capped { " (limited)" } else { "" }
                ),
            ),
            TableOutcome::Failed(error) => (MessageKind::Error, format!("{}: {error}", result.table)),
            TableOutcome::NotStarted => (MessageKind::Info, format!("{}: not started", result.table)),
        })
        .collect()
}

#[derive(Default)]
pub(crate) struct TablesExportState {
    pub open: bool,
    pub running: bool,
    pub focus: bool,
    pub folder: Option<Entity<TextInput>>,
    pub limit: Option<Entity<TextInput>>,
}

impl GridView {
    pub(crate) fn tables_export_actions(&self) -> Vec<Action> {
        let supported = self.live.as_ref().is_some_and(|session| {
            drivers_registry::default_registry()
                .get(&session.driver_id)
                .is_some_and(|driver| driver.capabilities().supports_snapshot_export)
        });
        vec![Action {
            id: "export-all-tables",
            label: "Export every table as CSV into a folder…".to_string(),
            enabled: supported,
        }]
    }

    pub(crate) fn focus_tables_export_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.search.tables_export.focus) {
            return;
        }
        if let Some(input) = &self.search.tables_export.folder {
            window.focus(&input.read(cx).focus_handle(cx), cx);
        }
    }

    fn tables_export_input(&mut self, hint: &str, cx: &mut Context<Self>) -> Entity<TextInput> {
        let input = cx.new(|cx| TextInput::new(hint, cx));
        cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Submit) {
                this.start_tables_export(cx);
            }
        })
        .detach();
        input
    }

    pub(crate) fn open_tables_export_bar(&mut self, cx: &mut Context<Self>) {
        if !self.tables_export_actions().iter().all(|action| action.enabled) {
            self.status = "Exporting every table needs a connection that can read a consistent snapshot".into();
            return;
        }
        if self.search.tables_export.folder.is_none() {
            let folder = self.tables_export_input("Absolute path of an existing folder", cx);
            let limit = self.tables_export_input("Rows per table (empty for all)", cx);
            self.search.tables_export.folder = Some(folder);
            self.search.tables_export.limit = Some(limit);
        }
        for (input, text) in [
            (self.search.tables_export.folder.clone(), ""),
            (self.search.tables_export.limit.clone(), "100000"),
        ] {
            if let Some(input) = input {
                input.update(cx, |input, cx| input.set_content(text, cx));
            }
        }
        self.search.tables_export.open = true;
        self.search.tables_export.focus = true;
        self.status = "Type the folder for one CSV file per table, then press Enter".into();
        cx.notify();
    }

    pub(crate) fn start_tables_export(&mut self, cx: &mut Context<Self>) {
        let (Some(session), Some(folder), Some(limit)) = (
            self.live.clone(),
            self.search.tables_export.folder.clone(),
            self.search.tables_export.limit.clone(),
        ) else {
            return;
        };
        if self.search.tables_export.running {
            self.status = "A table export is already running".into();
            cx.notify();
            return;
        }
        let parsed = export_folder(folder.read(cx).content().as_ref())
            .and_then(|directory| row_limit(limit.read(cx).content().as_ref()).map(|rows| (directory, rows)));
        let (directory, rows) = match parsed {
            Ok(parsed) => parsed,
            Err(message) => {
                self.status = message;
                cx.notify();
                return;
            }
        };
        self.search.tables_export.open = false;
        self.search.tables_export.running = true;
        self.status = "Exporting every table…".into();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    session.export_all_tables(directory, rows, &tokio_util::sync::CancellationToken::new())
                })
                .await;
            this.update(cx, |this, cx| {
                this.search.tables_export.running = false;
                match result {
                    Ok(results) => {
                        for (kind, line) in export_report(&results).into_iter().rev() {
                            this.panel.messages.push(kind, line);
                        }
                        this.tabs.open_query();
                        this.sync_grid_owner();
                        this.panel.view = ResultView::Messages;
                        this.panel.open = true;
                        this.status = format!("Exported {} table(s); see Messages", results.len());
                    }
                    Err(message) => this.status = format!("Could not export the tables: {message}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn tables_export_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let (folder, limit) = (
            self.search.tables_export.folder.clone()?,
            self.search.tables_export.limit.clone()?,
        );
        if !self.search.tables_export.open {
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
                .child(div().font_weight(gpui::FontWeight::MEDIUM).child("Export all tables"))
                .child(div().flex_1().min_w_0().child(folder))
                .child(div().w(px(size::FORM_CONTROL * 3.0)).child(limit))
                .child(
                    ghost("run-tables-export", "Export", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.start_tables_export(cx))),
                )
                .child(
                    ghost("cancel-tables-export", "Cancel", theme).on_click(cx.listener(|this, _, _, cx| {
                        this.search.tables_export.open = false;
                        cx.notify();
                    })),
                ),
        )
    }
}

#[cfg(test)]
#[path = "tables_export_ui_tests.rs"]
mod tests;
