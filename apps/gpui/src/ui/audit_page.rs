use bookie_app_services::audit_view::{self, OperationRow};
use bookie_app_services::service_runtime::block_on;
use bookie_gpui::theme::{Theme, size};
use gpui::{Context, Div, Entity, Stateful, div, prelude::*, px, rgb};

use crate::GridView;
use crate::assets::icon;
use crate::surfaces::{ListState, list_row, list_state, truncated};
use crate::text_input::{InputEvent, TextInput};
use bookie_gpui::messages::{MAX_STATEMENT_CHARS, one_line_sql};

const RECENT_RECORDS: usize = 1_000;

pub(crate) struct AuditPage {
    pub search: Entity<TextInput>,
    pub rows: Vec<OperationRow>,
    pub loading: bool,
    pub error: Option<String>,
    requests: crate::Ticket,
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
            requests: crate::Ticket::new(),
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
        let generation = self.audit_page.requests.next();
        self.audit_page.loading = true;
        cx.spawn(async move |this, cx| {
            let rows = cx
                .background_executor()
                .spawn(async move { block_on(audit_view::recent_operations(RECENT_RECORDS)) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !this.audit_page.requests.is_current(generation) {
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
            ("x", theme.destructive)
        } else {
            ("circle", theme.success)
        };
        let status = audit_view::status_text(row.status);
        list_row(("audit-row", index), false, theme)
            .items_start()
            .child(div().pt_1().child(icon(mark, color)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        truncated(("audit-sql", index), one_line_sql(&row.sql, MAX_STATEMENT_CHARS))
                            .text_size(px(size::TEXT_SMALL))
                            .font_family("monospace"),
                    )
                    .child(
                        truncated(("audit-meta", index), format!("{status} · {}", meta(row)))
                            .text_size(px(size::TEXT_SMALL))
                            .text_color(rgb(theme.muted_foreground)),
                    )
                    .children(row.error.as_ref().map(|error| {
                        div()
                            .text_size(px(size::TEXT_SMALL))
                            .text_color(rgb(theme.destructive))
                            .child(one_line_sql(error, MAX_STATEMENT_CHARS))
                    })),
            )
    }

    pub(crate) fn audit_page(&mut self, theme: Theme, _cx: &mut Context<Self>) -> Div {
        let query = self.audit_page.search.read(_cx).content().to_string();
        let rows = shown(&self.audit_page.rows, &query);
        let body = if let Some(error) = &self.audit_page.error {
            list_state(ListState::Failed(error), theme)
        } else if self.audit_page.loading && self.audit_page.rows.is_empty() {
            list_state(ListState::Loading("Reading the audit journal…"), theme)
        } else if rows.is_empty() {
            list_state(ListState::Empty("No operations match"), theme)
        } else {
            div().flex().flex_col().gap_1().children(
                rows.iter()
                    .enumerate()
                    .map(|(index, row)| self.audit_row(index, row, theme)),
            )
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
            .child(
                div()
                    .id("audit-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_app_services::audit_view::{AuditApprovalOutcome, AuditOperationClass, AuditTerminalStatus};

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
