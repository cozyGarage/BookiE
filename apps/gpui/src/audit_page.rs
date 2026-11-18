use bookie_app_services::audit_view::{self, OperationRow};
use bookie_gpui::theme::{Theme, size};
use gpui::{Context, Div, Entity, Stateful, div, prelude::*, px, rgb};

use crate::GridView;
use crate::assets::icon;
use crate::chrome::one_line;
use crate::history_view::block_on_history;
use crate::text_input::{InputEvent, TextInput};

const RECENT_RECORDS: usize = 1_000;

pub(crate) struct AuditPage {
    pub search: Entity<TextInput>,
    pub rows: Vec<OperationRow>,
    pub loading: bool,
    pub error: Option<String>,
    generation: u64,
}

impl AuditPage {
    pub fn new(cx: &mut Context<GridView>) -> Self {
        let search = cx.new(|cx| TextInput::new("Filter by operation id, connection, principal, kind or outcome", cx));
        cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Changed) {
                cx.notify();
            }
        })
        .detach();
        Self {
            search,
            rows: Vec::new(),
            loading: false,
            error: None,
            generation: 0,
        }
    }
}

fn meta(row: &OperationRow) -> String {
    let mut parts = vec![row.started.clone(), audit_view::class_text(row.class).to_string()];
    if let Some(rows) = row.rows {
        parts.push(format!("{rows} rows"));
    }
    if let Some(skipped) = row.rows_skipped {
        parts.push(format!("{skipped} skipped"));
    }
    if let Some(ms) = row.duration_ms {
        parts.push(format!("{ms} ms"));
    }
    parts.push(row.principal.clone());
    parts.push(audit_view::approval_text(row.approval).to_string());
    parts.push(row.connection.clone());
    parts.join(" · ")
}

fn shown<'a>(rows: &'a [OperationRow], query: &str) -> Vec<&'a OperationRow> {
    rows.iter()
        .filter(|row| {
            audit_view::matches(
                row,
                query,
                audit_view::class_text(row.class),
                audit_view::status_text(row.status),
            )
        })
        .collect()
}

impl GridView {
    pub(crate) fn reload_audit_page(&mut self, cx: &mut Context<Self>) {
        self.audit_page.generation += 1;
        self.audit_page.loading = true;
        let generation = self.audit_page.generation;
        cx.spawn(async move |this, cx| {
            let rows = cx
                .background_executor()
                .spawn(async move { block_on_history(audit_view::recent_operations(RECENT_RECORDS)) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.audit_page.generation != generation {
                    return;
                }
                this.audit_page.loading = false;
                match rows {
                    Ok(rows) => {
                        this.audit_page.rows = rows;
                        this.audit_page.error = None;
                    }
                    Err(message) => this.audit_page.error = Some(message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn audit_row(&self, index: usize, row: &OperationRow, theme: Theme) -> Stateful<Div> {
        let attention = audit_view::row_needs_attention(row);
        let (mark, color) = if attention {
            ("x", theme.warning)
        } else {
            ("circle", theme.success)
        };
        let status = audit_view::status_text(row.status);
        div()
            .id(("audit-row", index))
            .flex()
            .items_start()
            .gap_2()
            .px_3()
            .py_1()
            .rounded(px(theme.radius))
            .hover(move |style| style.bg(rgb(theme.accent)))
            .child(div().pt_1().child(icon(mark, color)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .overflow_hidden()
                            .text_size(px(size::TEXT_SMALL))
                            .font_family("monospace")
                            .child(one_line(&row.sql)),
                    )
                    .child(
                        div()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .overflow_hidden()
                            .text_size(px(size::TEXT_SMALL))
                            .text_color(rgb(theme.muted_foreground))
                            .child(format!("{status} · {}", meta(row))),
                    )
                    .children(row.error.as_ref().map(|error| {
                        div()
                            .text_size(px(size::TEXT_SMALL))
                            .text_color(rgb(theme.warning))
                            .child(one_line(error))
                    })),
            )
    }

    pub(crate) fn audit_page(&mut self, theme: Theme, _cx: &mut Context<Self>) -> Div {
        let query = self.audit_page.search.read(_cx).content().to_string();
        let rows = shown(&self.audit_page.rows, &query);
        let body = if let Some(error) = &self.audit_page.error {
            div().p_4().text_color(rgb(theme.warning)).child(error.clone())
        } else if self.audit_page.loading && self.audit_page.rows.is_empty() {
            div()
                .p_4()
                .text_color(rgb(theme.muted_foreground))
                .child("Reading the audit journal…")
        } else if rows.is_empty() {
            div()
                .p_4()
                .text_color(rgb(theme.muted_foreground))
                .child("No operations match")
        } else {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .children(rows.iter().enumerate().map(|(index, row)| self.audit_row(index, row, theme)))
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_3()
            .p_6()
            .bg(rgb(theme.background))
            .child(div().h(px(size::FORM_CONTROL)).child(self.audit_page.search.clone()))
            .child(div().id("audit-list").flex_1().min_h_0().overflow_y_scroll().child(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_policy::{AuditApprovalOutcome, AuditOperationClass, AuditTerminalStatus};

    fn row(sql: &str, status: AuditTerminalStatus) -> OperationRow {
        OperationRow {
            operation_id: uuid::Uuid::nil(),
            started: "2026-11-18 12:00:00".into(),
            connection: "shop".into(),
            principal: "human:gui".into(),
            class: AuditOperationClass::Mutation,
            status,
            approval: AuditApprovalOutcome::Approved,
            rule: String::new(),
            sql: sql.into(),
            error: None,
            rows: Some(3),
            rows_skipped: None,
            duration_ms: Some(12),
        }
    }

    #[test]
    fn a_row_leads_with_when_and_what_and_ends_with_the_long_connection_name() {
        assert_eq!(
            meta(&row("DELETE FROM t", AuditTerminalStatus::Succeeded)),
            "2026-11-18 12:00:00 · Data change · 3 rows · 12 ms · human:gui · approved · shop"
        );
    }

    #[test]
    fn the_filter_keeps_rows_matching_sql_or_outcome() {
        let rows = vec![
            row("DELETE FROM t", AuditTerminalStatus::Succeeded),
            row("SELECT 1", AuditTerminalStatus::Denied),
        ];
        assert_eq!(shown(&rows, "").len(), 2);
        assert_eq!(shown(&rows, "denied").len(), 1);
        assert_eq!(shown(&rows, "delete").len(), 1);
    }
}
