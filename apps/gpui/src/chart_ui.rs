use std::rc::Rc;
use std::sync::Arc;

use bookie_app_services::result_chart::{self, Aggregate, Kind, Settings, nice_ticks, tick_format};
use bookie_gpui::chart_source::{ChartState, MAX_CHART_BYTES};
use bookie_gpui::preview::text_preview;
use bookie_gpui::shell::ResultView;
use bookie_gpui::theme::{Theme, size};
use gpui::{Hsla, SharedString};
use gpui_kit::component::ActiveTheme;

use super::*;
use crate::chart_plot::{CHAR_WIDTH, ResultPlot, ValueAxis, min_width, number};
use crate::surfaces::{ButtonKind, button, segment, segmented};

const TICKS: usize = 5;

impl GridView {
    pub(crate) fn clear_chart(&mut self) {
        self.panel.chart = None;
        self.panel.chart_loading = false;
        self.panel.chart_error = None;
        if self.panel.view == ResultView::Chart {
            self.panel.view = ResultView::Table;
        }
    }

    pub(crate) fn show_chart(&mut self, cx: &mut Context<Self>) {
        self.panel.view = ResultView::Chart;
        if self.panel.chart.is_some() || self.panel.chart_loading {
            return;
        }
        self.panel.chart_error = None;
        let source = self.source.clone();
        self.begin_background(
            Work::Chart,
            cx,
            move |cancel| ChartState::load(source.as_ref(), &cancel),
            |this, result| {
                this.panel.chart_loading = false;
                match result {
                    Ok(chart) => this.panel.chart = Some(chart),
                    Err(error) => this.panel.chart_error = Some(error.message()),
                }
            },
        );
        self.panel.chart_loading = true;
    }

    pub(crate) fn edit_chart(&mut self, edit: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        let Some(chart) = self.panel.chart.as_mut() else { return };
        edit(&mut chart.settings);
        let rows = chart.rows.clone();
        let profiles = chart.profiles.clone();
        let settings = chart.settings.clone();
        let rows_cut = chart.data.rows_cut;
        self.begin_background(
            Work::Chart,
            cx,
            move |cancel| {
                if cancel.is_cancelled() {
                    return None;
                }
                let mut data = result_chart::compute(rows.as_ref(), &profiles, &settings);
                data.rows_cut = rows_cut;
                (!cancel.is_cancelled()).then_some(data)
            },
            |this, data| {
                this.panel.chart_loading = false;
                if let (Some(chart), Some(data)) = (this.panel.chart.as_mut(), data) {
                    chart.data = Arc::new(data);
                }
            },
        );
        self.panel.chart_loading = true;
        cx.notify();
    }

    pub(crate) fn chart_body(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let Some(chart) = self.panel.chart.as_ref() else {
            let message = if self.panel.chart_loading {
                "Preparing chart…"
            } else {
                self.panel.chart_error.unwrap_or("Choose Chart to plot this result")
            };
            return div().flex_1().p_4().child(message);
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.chart_controls(chart, theme, cx))
            .child(
                div()
                    .debug_selector(|| "chart-coverage".into())
                    .px_2()
                    .py_1()
                    .text_size(px(size::TEXT_SMALL))
                    .text_color(rgb(theme.muted_foreground))
                    .child(chart_coverage(chart, self.panel.chart_loading)),
            )
            .child(chart_plot_area(chart, theme, cx))
    }

    fn chart_controls(&self, chart: &ChartState, theme: Theme, cx: &mut Context<Self>) -> Div {
        let settings = &chart.settings;
        let pick = |id: gpui::ElementId, label: &str, on: bool| {
            segment(id, label.to_string(), on, theme).max_w(px(size::CHART_COLUMN_WIDTH))
        };
        let kinds =
            [("chart-bars", "Bars", Kind::Bars), ("chart-line", "Line", Kind::Line)].map(|(id, label, kind)| {
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
        let label_name = chart
            .columns
            .get(settings.label)
            .map(|name| text_preview(name))
            .unwrap_or_default();
        let label = button(ButtonKind::Ghost, "chart-label", format!("Label: {label_name}"), theme)
            .max_w(px(size::CHART_COLUMN_WIDTH))
            .on_click(cx.listener(move |this, _, _, cx| this.edit_chart(|s| s.label = (s.label + 1) % columns, cx)));
        let values = settings.picked().map(|(_, column)| {
            let name = chart
                .columns
                .get(column)
                .map(|name| text_preview(name))
                .unwrap_or_default();
            pick(
                ("chart-value", column).into(),
                &name,
                settings.slot_of(column).is_some(),
            )
            .debug_selector(move || format!("chart-value-{column}"))
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
            .child(segmented(kinds, theme))
            .child(div().w(px(size::SM)))
            .child(segmented(aggregates, theme))
            .child(div().w(px(size::SM)))
            .child(label)
            .child(
                button(ButtonKind::Ghost, "chart-values", "Values…", theme)
                    .debug_selector(|| "chart-values-picker".into())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_picker(crate::palette_state::PaletteMode::ChartValues, window, cx);
                    })),
            )
            .child(segmented(values, theme).flex_shrink(1.0).flex_wrap().h_auto())
    }
}

fn chart_coverage(chart: &ChartState, loading: bool) -> String {
    let data = &chart.data;
    let mut text = format!(
        "{} of {} groups · {} of {} retained rows",
        data.labels.len(),
        data.groups,
        data.rows,
        chart.total,
    );
    if data.rows_cut {
        text.push_str(&format!(
            " · Sampled ({} rows / estimated {} MiB)",
            result_chart::MAX_ROWS,
            MAX_CHART_BYTES / (1024 * 1024)
        ));
    }
    if chart.incomplete {
        text.push_str(" · Query result is incomplete");
    }
    if loading {
        text.push_str(" · Updating chart…");
    }
    text
}

fn chart_plot_area(chart: &ChartState, theme: Theme, cx: &App) -> Div {
    let data = chart.data.clone();
    let kind = chart.settings.kind;
    let note = |text: &str| {
        div()
            .flex_1()
            .p_4()
            .text_color(rgb(theme.muted_foreground))
            .child(text.to_string())
    };
    if data.rows == 0 {
        return note("No rows are available within the chart preview limits");
    }
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
    let palette = [
        component.chart_1,
        component.chart_2,
        component.chart_3,
        component.chart_4,
        component.chart_5,
    ];
    let colors: Rc<[Hsla]> = data
        .series
        .iter()
        .map(|series| palette[series.slot % palette.len()])
        .collect();
    let names: Rc<[SharedString]> = data
        .series
        .iter()
        .map(|series| {
            series
                .column
                .and_then(|column| chart.columns.get(column).map(|name| text_preview(name)))
                .unwrap_or_else(|| "Rows".into())
                .into()
        })
        .collect();
    let labels: Rc<[SharedString]> = data
        .labels
        .iter()
        .map(|label| label.as_deref().map_or_else(|| "NULL".into(), text_preview).into())
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
                    .child(div().size(px(size::SM)).rounded(px(size::OPTICAL)).bg(*color))
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
    div().flex_1().min_h_0().flex().flex_col().children(legend).child(
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
