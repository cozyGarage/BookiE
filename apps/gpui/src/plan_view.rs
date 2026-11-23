use bookie_app_services::explain_plan::{PlanNode, misestimate, parse_plan, plan_text};
use bookie_gpui::model::CellText;
use bookie_gpui::source::ResultSource;

use super::*;
use bookie_gpui::shell::ResultView;
use bookie_gpui::theme::{Theme, size};

const PLAN_ROW_CAP: u64 = 2_000;

impl GridView {
    pub(crate) fn show_plan_of(&mut self, table: &impl ResultSource) {
        let explaining = std::mem::take(&mut self.panel.explaining);
        self.panel.plan = if explaining { plan_nodes(table) } else { Vec::new() };
        if !self.panel.plan.is_empty() {
            self.panel.view = ResultView::Plan;
        } else if self.panel.view == ResultView::Plan {
            self.panel.view = ResultView::Table;
        }
    }

    pub(crate) fn plan_body(&self, theme: Theme) -> Div {
        let heaviest = self
            .panel
            .plan
            .iter()
            .filter_map(|node| node.cost)
            .fold(0.0_f64, f64::max);
        div().flex_1().min_h_0().child(
            div()
                .id("plan-body")
                .size_full()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2()
                .p_4()
                .children(
                    self.panel
                        .plan
                        .iter()
                        .enumerate()
                        .map(|(index, node)| plan_row(node, misestimate(&self.panel.plan, index), heaviest, theme)),
                ),
        )
    }
}

fn plan_nodes(table: &impl ResultSource) -> Vec<PlanNode> {
    let columns: Vec<String> = table.columns().iter().map(|column| column.name.clone()).collect();
    let rows: Vec<Vec<String>> = (0..table.total().min(PLAN_ROW_CAP))
        .map(|index| table.row(index).iter().map(CellText::display).collect())
        .collect();
    parse_plan(&plan_text(&columns, &rows))
}

fn plan_row(node: &PlanNode, missed: Option<f64>, heaviest: f64, theme: Theme) -> Div {
    let bar = node.cost.filter(|_| heaviest > 0.0).map(|cost| {
        let share = (cost / heaviest).clamp(0.0, 1.0) as f32;
        div()
            .h(px(size::SM))
            .w_full()
            .rounded(px(theme.radius))
            .bg(rgb(theme.border))
            .child(
                div()
                    .h_full()
                    .w(gpui::relative(share))
                    .rounded(px(theme.radius))
                    .bg(rgb(theme.foreground)),
            )
    });
    let rows = node.rows.map(|rows| format!("~{rows:.0} rows"));
    let actual = node.actual_rows.map(|rows| format!("{rows:.0} actual"));
    let cost = node.cost.map(|cost| format!("cost {cost:.2}"));
    let stats: Vec<String> = cost.into_iter().chain(rows).chain(actual).collect();
    let warning = missed.map(|factor| {
        div()
            .debug_selector(|| "plan-misestimate".into())
            .text_size(px(size::TEXT_SMALL))
            .text_color(rgb(theme.warning))
            .child(format!("Row estimate off by {factor:.0}×"))
    });
    div()
        .flex()
        .flex_col()
        .gap_1()
        .ml(px(18.0 * node.depth as f32))
        .child(div().font_weight(gpui::FontWeight::MEDIUM).child(node.title.clone()))
        .children(bar)
        .children((!stats.is_empty()).then(|| {
            div()
                .text_size(px(size::TEXT_SMALL))
                .text_color(rgb(theme.muted_foreground))
                .child(stats.join(" · "))
        }))
        .children(warning)
        .children(node.details.iter().map(|detail| {
            div()
                .text_size(px(size::TEXT_SMALL))
                .text_color(rgb(theme.muted_foreground))
                .child(detail.clone())
        }))
}
