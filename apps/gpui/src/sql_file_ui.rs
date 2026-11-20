use std::path::PathBuf;

use bookie_app_services::sql_file::{file_error_message, read_sql_file, save_sql_file};

use super::*;
use crate::backup_ui::absolute_path;
use crate::chrome::ghost;

pub(crate) fn sql_file_destination(typed: &str) -> Result<PathBuf, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err("Type where to save the script".into());
    }
    let mut path = absolute_path(typed)?;
    if typed.ends_with('/') || path.file_name().is_none() {
        return Err("Type a file name for the script, not a folder".into());
    }
    if path.extension().is_none() {
        path.set_extension("sql");
    }
    if path.exists() {
        return Err("That file already exists; choose a new file name".into());
    }
    Ok(path)
}

pub(crate) fn sql_file_source(typed: &str) -> Result<PathBuf, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err("Type the path of the SQL file".into());
    }
    let path = absolute_path(typed)?;
    if path.is_file() {
        Ok(path)
    } else {
        Err("That SQL file does not exist".into())
    }
}

#[derive(Default)]
pub(crate) struct SqlFileState {
    pub open: bool,
    pub saving: bool,
    pub focus: bool,
    pub input: Option<Entity<TextInput>>,
    pub opened: Option<String>,
}

impl GridView {
    pub(crate) fn sql_file_actions(&self) -> Vec<Action> {
        vec![
            Action {
                id: "open-sql-file",
                label: "Open a SQL file into the editor…".to_string(),
                enabled: true,
            },
            Action {
                id: "save-sql-file",
                label: "Save the editor text to a SQL file…".to_string(),
                enabled: true,
            },
        ]
    }

    pub(crate) fn apply_opened_sql(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.search.sql_file.opened.take() else {
            return;
        };
        self.sql_input.update(cx, |input, cx| input.set_value(text, window, cx));
        self.focus_editor = true;
    }

    pub(crate) fn focus_sql_file_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.search.sql_file.focus) {
            return;
        }
        if let Some(input) = &self.search.sql_file.input {
            window.focus(&input.read(cx).focus_handle(cx), cx);
        }
    }

    pub(crate) fn open_sql_file_bar(&mut self, saving: bool, cx: &mut Context<Self>) {
        let input = self.search.sql_file.input.get_or_insert_with(|| {
            let input = cx.new(|cx| TextInput::new("Absolute path of the .sql file", cx));
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submit) {
                    this.run_sql_file(cx);
                }
            })
            .detach();
            input
        });
        input.update(cx, |input, cx| input.set_content("", cx));
        self.search.sql_file.open = true;
        self.search.sql_file.saving = saving;
        self.search.sql_file.focus = true;
        self.status = if saving {
            "Type where to save the editor text".into()
        } else {
            "Type the path of the SQL file to open".into()
        };
        cx.notify();
    }

    pub(crate) fn run_sql_file(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.search.sql_file.input.clone() else {
            return;
        };
        let typed = input.read(cx).content().to_string();
        let saving = self.search.sql_file.saving;
        let path = if saving {
            sql_file_destination(&typed)
        } else {
            sql_file_source(&typed)
        };
        let path = match path {
            Ok(path) => path,
            Err(message) => {
                self.status = message;
                cx.notify();
                return;
            }
        };
        let text = self.sql_input.read(cx).value().to_string();
        self.search.sql_file.open = false;
        self.status = if saving { "Saving…" } else { "Reading…" }.into();
        let shown = path.display().to_string();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if saving {
                        save_sql_file(&path, &text, None)
                            .map(|_| None)
                            .map_err(|error| file_error_message(&error))
                    } else {
                        read_sql_file(&path).map(|file| Some(file.text))
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                this.status = match result {
                    Ok(Some(text)) => {
                        this.search.sql_file.opened = Some(text);
                        this.tabs.open_query();
                        this.sync_grid_owner();
                        format!("Opened {shown}")
                    }
                    Ok(None) => format!("Saved the script to {shown}"),
                    Err(message) => message,
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn sql_file_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let input = self.search.sql_file.input.clone()?;
        if !self.search.sql_file.open {
            return None;
        }
        let saving = self.search.sql_file.saving;
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
                .child(div().font_weight(gpui::FontWeight::MEDIUM).child(if saving {
                    "Save SQL file"
                } else {
                    "Open SQL file"
                }))
                .child(div().flex_1().min_w_0().child(input))
                .child(
                    ghost("run-sql-file", if saving { "Save" } else { "Open" }, theme)
                        .on_click(cx.listener(|this, _, _, cx| this.run_sql_file(cx))),
                )
                .child(
                    ghost("cancel-sql-file", "Cancel", theme).on_click(cx.listener(|this, _, _, cx| {
                        this.search.sql_file.open = false;
                        cx.notify();
                    })),
                ),
        )
    }
}

#[cfg(test)]
#[path = "sql_file_ui_tests.rs"]
mod tests;
