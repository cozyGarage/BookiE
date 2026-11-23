use std::rc::Rc;

use bookie_app_services::result_chart::{self, Aggregate, ChartData, Kind, Profile, Settings, nice_ticks, tick_format};
use bookie_gpui::model::Cell;
use bookie_gpui::shell::ResultView;
use bookie_gpui::theme::{Theme, size};
use gpui::{Hsla, SharedString};
use gpui_kit::component::ActiveTheme;

use super::*;
use crate::chart_plot::{CHAR_WIDTH, ResultPlot, ValueAxis, min_width, number};

const TICKS: usize = 5;

pub(crate) struct ChartState {
    pub settings: Settings,
    pub profiles: Vec<Profile>,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
    pub data: Rc<ChartData>,
}

impl ChartState {
    pub(crate) fn from_rows(columns: Vec<bookie_core::ColumnInfo>, rows: Vec<Vec<Cell>>) -> Self {
        let profiles = result_chart::profile(&columns, &rows);
        let settings = result_chart::defaults(&profiles);
        let data = Rc::new(result_chart::compute(&rows, &profiles, &settings));
        Self {
            settings,
            profiles,
            columns: columns.into_iter().map(|column| column.name).collect(),
            rows,
            data,
        }
    }

    pub(crate) fn change(&mut self, edit: impl FnOnce(&mut Settings)) {
        edit(&mut self.settings);
        self.data = Rc::new(result_chart::compute(&self.rows, &self.profiles, &self.settings));
    }
}

impl GridView {
    pub(crate) fn show_chart(&mut self) {
        let columns = column_infos_for(self.source.columns());
        let total = self.source.total().min(result_chart::MAX_ROWS as u64);
        let rows = (0..total).map(|index| self.source.row(index)).collect();
        self.panel.chart = Some(ChartState::from_rows(columns, rows));
        self.panel.view = ResultView::Chart;
    }

    fn edit_chart(&mut self, edit: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        if let Some(chart) = self.panel.chart.as_mut() {
            chart.change(edit);
        }
        cx.notify();
    }

    pub(crate) fn chart_body(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let Some(chart) = self.panel.chart.as_ref() else {
            return div().flex_1().p_4().child("Choose Chart to plot this result");
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.chart_controls(chart, theme, cx))
            .child(chart_plot_area(chart, theme, cx))
    }

    fn chart_controls(&self, chart: &ChartState, theme: Theme, cx: &mut Context<Self>) -> Div {
        let settings = &chart.settings;
        let pick = |id: gpui::ElementId, label: &str, on: bool| chip(id, label, on, theme);
        let kinds = [("chart-bars", "Bars", Kind::Bars), ("chart-line", "Line", Kind::Line)].map(|(id, label, kind)| {
            pick(id.into(), label, settings.kind == kind)
                .on_click(cx.listener(move |this, _, _, cx| this.edit_chart(|s| s.kind = kind, cx)))
        });
        let aggregates = [
            ("chart-sum", "Sum", Aggregate::Sum),
            ("chart-count", "Count", Aggregate::Count),
            ("chart-average", "Average", Aggregate::Average),
        ]
        .map(|(id, label, aggregate)| {
            pick(id.into(), label, settings.aggregate == aggregate)
                .on_click(cx.listener(move |this, _, _, cx| this.edit_chart(|s| s.aggregate = aggregate, cx)))
        });
        let columns = chart.columns.len().max(1);
        let label_name = chart.columns.get(settings.label).cloned().unwrap_or_default();
        let label = pick("chart-label".into(), &format!("Label: {label_name}"), false).on_click(cx.listener(
            move |this, _, _, cx| this.edit_chart(|s| s.label = (s.label + 1) % columns, cx),
        ));
        let values = chart
            .profiles
            .iter()
            .enumerate()
            .filter(|(_, profile)| profile.numeric)
            .map(|(column, _)| {
                let name = chart.columns.get(column).cloned().unwrap_or_default();
                pick(("chart-value", column).into(), &name, settings.slot_of(column).is_some())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_chart(
                            |s| {
                                s.toggle_value(column);
                            },
                            cx,
                        )
                    }))
            });
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .children(kinds)
            .child(div().w(px(size::SM)))
            .children(aggregates)
            .child(div().w(px(size::SM)))
            .child(label)
            .child(div().text_color(rgb(theme.muted_foreground)).child("Values:"))
            .children(values)
    }
}

fn chip(id: gpui::ElementId, label: &str, on: bool, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(size::COMPACT_CONTROL))
        .px_2()
        .rounded(px(size::CONTROL_RADIUS))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(if on { theme.active_row } else { theme.background }))
        .text_color(rgb(theme.foreground))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(theme.accent)))
        .child(label.to_string())
}

fn chart_plot_area(chart: &ChartState, theme: Theme, cx: &App) -> Div {
    let data = chart.data.clone();
    let kind = chart.settings.kind;
    let note = |text: &str| div().flex_1().p_4().text_color(rgb(theme.muted_foreground)).child(text.to_string());
    if data.series.is_empty() {
        return note("Pick a number column under Values to plot it");
    }
    let Some((lo, hi)) = data.range(kind) else {
        return note("The chosen columns have no numbers to plot");
    };
    let ticks: Rc<[f64]> = nice_ticks(lo, hi, TICKS).into();
    let reach = ticks.iter().fold(0f64, |reach, tick| reach.max(tick.abs()));
    let step = ticks.get(1).zip(ticks.first()).map_or(1.0, |(b, a)| b - a);
    let (unit, suffix, decimals) = tick_format(reach, step);
    let tick_labels: Rc<[SharedString]> = ticks
        .iter()
        .map(|&tick| {
            if tick == 0.0 {
                SharedString::from("0")
            } else {
                format!("{}{suffix}", number(tick / unit, decimals)).into()
            }
        })
        .collect();
    let font = px(size::TEXT_SMALL);
    let gutter = tick_labels
        .iter()
        .map(|label| label.len() as f32 * size::TEXT_SMALL * CHAR_WIDTH)
        .fold(0.0, f32::max)
        + 12.0;
    let component = cx.theme();
    let palette = [component.chart_1, component.chart_2, component.chart_3, component.chart_4, component.chart_5];
    let colors: Rc<[Hsla]> = data.series.iter().map(|series| palette[series.slot % palette.len()]).collect();
    let names: Rc<[SharedString]> = data
        .series
        .iter()
        .map(|series| {
            series
                .column
                .and_then(|column| chart.columns.get(column).cloned())
                .unwrap_or_else(|| "Rows".into())
                .into()
        })
        .collect();
    let labels: Rc<[SharedString]> = data
        .labels
        .iter()
        .map(|label| label.clone().unwrap_or_else(|| "NULL".into()).into())
        .collect();
    let width = min_width(labels.len(), data.series.len(), kind);
    let legend = (names.len() > 1).then(|| {
        div()
            .flex()
            .flex_wrap()
            .gap_3()
            .px_2()
            .text_size(px(size::TEXT_SMALL))
            .text_color(rgb(theme.muted_foreground))
            .children(names.iter().zip(colors.iter()).map(|(name, color)| {
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().size(px(size::SM)).rounded(px(2.0)).bg(*color))
                    .child(name.clone())
            }))
    });
    let plot = ResultPlot {
        data: data.clone(),
        kind,
        labels,
        names,
        colors,
        ticks: ticks.clone(),
        font,
    };
    let axis = ValueAxis {
        ticks,
        labels: tick_labels,
        font,
    };
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .children(legend)
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .pr_4()
                .pb_2()
                .child(div().flex_none().w(px(gutter)).h_full().child(axis))
                .child(
                    div()
                        .id("chart-plot")
                        .debug_selector(|| "chart-plot".into())
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .overflow_x_scroll()
                        .child(div().h_full().w_full().min_w(px(width)).child(plot)),
                ),
        )
}

#[cfg(test)]
#[path = "chart_ui_tests.rs"]
mod tests;
