use std::io::Write;
use std::path::{Path, PathBuf};

use bookie_app_services::er_diagram::SchemaDiagram;
use bookie_app_services::er_diagram::export::{dbml, mermaid};

use super::*;
use crate::chrome::ghost;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiagramFormat {
    Mermaid,
    Dbml,
}

pub(crate) fn format_for_path(path: &Path) -> Option<DiagramFormat> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "mmd" | "mermaid" => Some(DiagramFormat::Mermaid),
        "dbml" => Some(DiagramFormat::Dbml),
        _ => None,
    }
}

pub(crate) fn diagram_text(format: DiagramFormat, diagram: &SchemaDiagram) -> String {
    match format {
        DiagramFormat::Mermaid => mermaid(diagram),
        DiagramFormat::Dbml => dbml(diagram),
    }
}

fn write_new_file(path: &Path, text: &str) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.write_all(text.as_bytes()).map_err(|error| error.to_string())
}

#[derive(Default)]
pub(crate) struct SchemaDiagramState {
    pub open: bool,
    pub focus: bool,
    pub input: Option<Entity<TextInput>>,
}

impl GridView {
    pub(crate) fn schema_diagram_actions(&self) -> Vec<Action> {
        let enabled = self.live.as_ref().is_some_and(|session| !session.tables.is_empty());
        vec![Action {
            id: "export-schema-diagram",
            label: "Export schema diagram (Mermaid or DBML)…".to_string(),
            enabled,
        }]
    }

    pub(crate) fn focus_schema_diagram_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.search.diagram.focus) {
            return;
        }
        if let Some(input) = &self.search.diagram.input {
            window.focus(&input.read(cx).focus_handle(cx), cx);
        }
    }

    pub(crate) fn open_schema_diagram_bar(&mut self, cx: &mut Context<Self>) {
        if self.live.is_none() {
            self.status = "Connect to a database to export its schema diagram".into();
            return;
        }
        let input = self.search.diagram.input.get_or_insert_with(|| {
            let input = cx.new(|cx| TextInput::new("New file ending in .mmd, .mermaid or .dbml", cx));
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Submit) {
                    this.write_schema_diagram(cx);
                }
            })
            .detach();
            input
        });
        input.update(cx, |input, cx| input.set_content("", cx));
        self.search.diagram.open = true;
        self.search.diagram.focus = true;
        self.status = "Type where to write the diagram of every table and its foreign keys".into();
        cx.notify();
    }

    fn diagram_destination(&mut self, typed: &str) -> Option<(PathBuf, DiagramFormat)> {
        let path = bookie_core::user_path::expand_tilde(Path::new(typed));
        let Some(format) = format_for_path(&path) else {
            self.status = "Use a file name ending in .mmd, .mermaid or .dbml".into();
            return None;
        };
        if typed.is_empty() || path.exists() {
            self.status = "That file already exists or no path was given; choose a new file name".into();
            return None;
        }
        Some((path, format))
    }

    pub(crate) fn write_schema_diagram(&mut self, cx: &mut Context<Self>) {
        let (Some(session), Some(input)) = (self.live.clone(), self.search.diagram.input.clone()) else {
            return;
        };
        let typed = input.read(cx).content().trim().to_string();
        let Some((path, format)) = self.diagram_destination(&typed) else {
            return;
        };
        self.status = "Reading every table…".into();
        let shown = path.display().to_string();
        self.begin_background(
            Work::Load,
            cx,
            move |cancel| {
                let diagram = session.schema_diagram(&cancel)?;
                write_new_file(&path, &diagram_text(format, &diagram))?;
                Ok::<usize, String>(diagram.tables.len())
            },
            move |this, result| match result {
                Ok(tables) => {
                    this.search.diagram.open = false;
                    this.status = format!("Wrote the diagram of {tables} table(s) to {shown}");
                }
                Err(message) => this.status = format!("Could not export the diagram: {message}"),
            },
        );
    }

    pub(crate) fn schema_diagram_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let input = self.search.diagram.input.clone()?;
        if !self.search.diagram.open {
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
                .child(
                    div()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child("Export schema diagram"),
                )
                .child(div().flex_1().min_w_0().child(input))
                .child(
                    ghost("write-schema-diagram", "Export", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.write_schema_diagram(cx))),
                )
                .child(
                    ghost("cancel-schema-diagram", "Cancel", theme).on_click(cx.listener(|this, _, _, cx| {
                        this.search.diagram.open = false;
                        cx.notify();
                    })),
                ),
        )
    }
}

#[cfg(test)]
#[path = "schema_diagram_ui_tests.rs"]
mod tests;
