use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use bookie_app_services::er_diagram::SchemaDiagram;
use bookie_app_services::er_diagram::export::{dbml, mermaid};

use crate::surfaces::{ButtonKind, button, task_bar};
use crate::*;

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

const EXPORT_RUNNING: u8 = 0;
const EXPORT_CANCELLED: u8 = 1;
const EXPORT_PUBLISHING: u8 = 2;

fn write_new_file(path: &Path, text: &str, phase: &AtomicU8) -> Result<(), String> {
    bookie_core::export::write_atomically_no_replace_checked(
        path,
        |file| file.write_all(text.as_bytes()),
        || {
            phase
                .compare_exchange(EXPORT_RUNNING, EXPORT_PUBLISHING, Ordering::AcqRel, Ordering::Acquire)
                .map(|_| ())
                .map_err(|_| std::io::Error::other("Canceled the diagram export"))
        },
    )
    .map_err(|error| error.to_string())
}

#[derive(Default)]
pub(crate) struct SchemaDiagramState {
    pub request: crate::Ticket,
    pub loading: bool,
    pub failure: Option<String>,
    pub scope: Option<String>,
    pub relationship: Option<crate::diagram_relationship::RelationshipDraft>,
    pub ddl: Option<crate::diagram_ddl::DiagramDdl>,
    pub find: Option<Entity<TextInput>>,
    pub scroll: gpui::ScrollHandle,
    pub view: Option<crate::diagram_ui::DiagramView>,
    export_scope: Option<String>,
    pub open: bool,
    pub focus: bool,
    pub input: Option<Entity<TextInput>>,
    export_request: crate::Ticket,
    export_phase: Option<Arc<AtomicU8>>,
}

impl Drop for SchemaDiagramState {
    fn drop(&mut self) {
        if let Some(phase) = &self.export_phase {
            let _ = phase.compare_exchange(EXPORT_RUNNING, EXPORT_CANCELLED, Ordering::AcqRel, Ordering::Acquire);
        }
    }
}

impl GridView {
    pub(crate) fn schema_diagram_actions(&self) -> Vec<Action> {
        let enabled = self.live.as_ref().is_some_and(|session| !session.tables.is_empty());
        vec![Action {
            id: "export-schema-diagram",
            label: "Export schema diagram as Mermaid or DBML…".to_string(),
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
        self.search.diagram.export_scope = (self.tabs.current() == &Tab::Diagram)
            .then(|| self.search.diagram.scope.clone())
            .flatten();
        self.search.diagram.open = true;
        self.search.diagram.focus = true;
        self.status = "Choose a new file for the diagram export".into();
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
        if self.search.diagram.export_phase.is_some() {
            return;
        }
        let (Some(session), Some(input)) = (self.live.clone(), self.search.diagram.input.clone()) else {
            return;
        };
        let typed = input.read(cx).content().trim().to_string();
        let Some((path, format)) = self.diagram_destination(&typed) else {
            return;
        };
        self.status = "Reading schema…".into();
        let scope = self.search.diagram.export_scope.clone();
        let shown = path.display().to_string();
        self.search.diagram.export_request.invalidate();
        let generation = self.search.diagram.export_request.generation;
        let cancel = CancellationToken::new();
        self.search.diagram.export_request.cancel = cancel.clone();
        let phase = Arc::new(AtomicU8::new(EXPORT_RUNNING));
        self.search.diagram.export_phase = Some(phase.clone());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let read = session.schema_diagram(scope.as_deref(), &cancel)?;
                    if cancel.is_cancelled() {
                        return Err("Canceled the diagram export".to_string());
                    }
                    let incomplete = read.truncated() || !read.skipped.is_empty();
                    let diagram = SchemaDiagram::build(read.reads);
                    let text = diagram_text(format, &diagram);
                    if cancel.is_cancelled() {
                        return Err("Canceled the diagram export".to_string());
                    }
                    write_new_file(&path, &text, &phase)?;
                    Ok::<_, String>((diagram.tables.len(), read.total_tables, incomplete))
                })
                .await;
            this.update(cx, |this, cx| {
                if this.search.diagram.export_request.generation != generation {
                    return;
                }
                this.search.diagram.export_phase = None;
                match result {
                    Ok((tables, total, incomplete)) => {
                        this.search.diagram.open = false;
                        this.status = if incomplete {
                            format!("Wrote a partial diagram ({tables} of {total} tables) to {shown}")
                        } else {
                            format!("Wrote the diagram of {tables} table(s) to {shown}")
                        };
                        this.notices
                            .push(this.status.clone(), crate::notifications::NoticeKind::Success);
                    }
                    Err(message) => this.report_failure("export the diagram", &message),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn close_schema_diagram_export(&mut self) {
        if let Some(phase) = &self.search.diagram.export_phase {
            if phase
                .compare_exchange(EXPORT_RUNNING, EXPORT_CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                self.status = "The diagram export is finishing and can no longer be canceled".into();
                return;
            }
            self.search.diagram.export_request.invalidate();
            self.search.diagram.export_phase = None;
            self.status = "Canceled the diagram export".into();
        }
        self.search.diagram.open = false;
    }

    pub(crate) fn schema_diagram_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let input = self.search.diagram.input.clone()?;
        if !self.search.diagram.open {
            return None;
        }
        Some(
            task_bar("Export schema diagram", theme)
                .child(div().flex_1().min_w_0().child(input))
                .child(
                    button(ButtonKind::Primary, "write-schema-diagram", "Export", theme)
                        .on_click(cx.listener(|this, _, _, cx| this.write_schema_diagram(cx))),
                )
                .child(
                    button(ButtonKind::Ghost, "cancel-schema-diagram", "Cancel", theme).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.close_schema_diagram_export();
                            cx.notify();
                        },
                    )),
                ),
        )
    }
}

#[cfg(test)]
#[path = "schema_diagram_ui_tests.rs"]
mod tests;
