use std::rc::Rc;

use bookie_app_services::result_chart::{ChartData, Kind};
use bookie_gpui::theme::size;
use gpui::{AnyElement, App, Bounds, Corners, ElementId, Hsla, Pixels, Point, SharedString, TextAlign, Window, point, px};
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::plot::label::{PlotLabel, TEXT_GAP, Text, measure_text_width, truncate_text_to_width};
use gpui_kit::component::plot::shape::Line;
use gpui_kit::component::plot::tooltip::{CrossLine, Dot, Tooltip, TooltipState};
use gpui_kit::component::plot::{Curve, Grid, IntoPlot, Plot, axis_gutter};

const MAX_BAR: f32 = 36.;
const MIN_BAR: f32 = 6.;
const BAR_GAP: f32 = size::OPTICAL;
const BAR_RADIUS: f32 = 3.;
const GROUP_PAD: f32 = 0.2;
const MIN_POINT_GAP: f32 = 3.;
const DOTS: usize = 60;
const DOT: f32 = 6.;
const HOVER_DOT: f32 = size::SM;
const MAX_LABEL: f32 = 120.;
const LABEL_GAP: f32 = size::SM;
pub(crate) const CHAR_WIDTH: f32 = 0.62;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    pub top: f32,
    pub bottom: f32,
    lo: f64,
    hi: f64,
}

impl Frame {
    pub fn new(height: f32, font: f32, ticks: &[f64]) -> Self {
        let (lo, hi) = (ticks.first().copied().unwrap_or(0.), ticks.last().copied().unwrap_or(1.));
        let top = font / 2. + TEXT_GAP;
        Self {
            top,
            bottom: (height - axis_gutter(px(font))).max(top + 1.),
            lo,
            hi,
        }
    }

    pub fn y(&self, value: f64) -> f32 {
        let span = (self.hi - self.lo).max(f64::EPSILON);
        self.bottom - ((value - self.lo) / span) as f32 * (self.bottom - self.top)
    }
}

pub(crate) fn bar_layout(step: f32, series: usize) -> (f32, f32) {
    let count = series.max(1) as f32;
    let gap = if series > 1 { BAR_GAP } else { 0. };
    let room = step * (1. - GROUP_PAD) - gap * (count - 1.);
    ((room / count).clamp(1., MAX_BAR), gap)
}

pub(crate) fn min_width(groups: usize, series: usize, kind: Kind) -> f32 {
    let groups = groups as f32;
    match kind {
        Kind::Bars => {
            let count = series.max(1) as f32;
            groups * (count * MIN_BAR + (count - 1.) * BAR_GAP) / (1. - GROUP_PAD)
        }
        Kind::Line => groups * MIN_POINT_GAP,
    }
}

pub(crate) fn label_stride(step: f32, width: f32) -> usize {
    ((width + LABEL_GAP) / step.max(f32::EPSILON)).ceil().max(1.) as usize
}

pub(crate) fn nearest(x: f32, step: f32, groups: usize) -> usize {
    ((x / step.max(f32::EPSILON)).floor().max(0.) as usize).min(groups.saturating_sub(1))
}

pub(crate) fn number(value: f64, decimals: usize) -> String {
    format!("{value:.decimals$}")
}

#[derive(IntoPlot)]
pub(crate) struct ResultPlot {
    pub data: Rc<ChartData>,
    pub kind: Kind,
    pub labels: Rc<[SharedString]>,
    pub names: Rc<[SharedString]>,
    pub colors: Rc<[Hsla]>,
    pub ticks: Rc<[f64]>,
    pub font: Pixels,
}

impl ResultPlot {
    fn frame(&self, bounds: &Bounds<Pixels>) -> Frame {
        Frame::new(bounds.size.height.as_f32(), self.font.as_f32(), &self.ticks)
    }

    fn step(&self, bounds: &Bounds<Pixels>) -> f32 {
        bounds.size.width.as_f32() / self.data.labels.len().max(1) as f32
    }

    fn bar_x(&self, group: usize, series: usize, step: f32) -> (f32, f32) {
        let count = self.data.series.len();
        let (width, gap) = bar_layout(step, count);
        let span = width * count as f32 + gap * (count.max(1) - 1) as f32;
        (step * (group as f32 + 0.5) - span / 2. + series as f32 * (width + gap), width)
    }

    fn paint_bars(&self, frame: &Frame, step: f32, bounds: &Bounds<Pixels>, window: &mut Window) {
        let zero = frame.y(0.);
        for (k, series) in self.data.series.iter().enumerate() {
            let color = self.colors[k];
            for (group, value) in series.values.iter().enumerate() {
                let Some(value) = *value else { continue };
                let (x, width) = self.bar_x(group, k, step);
                let y = frame.y(value);
                let (top, height) = (y.min(zero), (y - zero).abs().max(1.));
                let radius = px((width / 2.).min(BAR_RADIUS));
                let corners = match value >= 0. {
                    true => Corners {
                        top_left: radius,
                        top_right: radius,
                        ..Default::default()
                    },
                    false => Corners {
                        bottom_left: radius,
                        bottom_right: radius,
                        ..Default::default()
                    },
                };
                let bar = Bounds::new(bounds.origin + point(px(x), px(top)), gpui::size(px(width), px(height)));
                window.paint_quad(gpui::fill(bar, color).corner_radii(corners));
            }
        }
    }

    fn paint_lines(&self, frame: &Frame, step: f32, bounds: &Bounds<Pixels>, window: &mut Window, cx: &App) {
        let dots = self.data.labels.len() <= DOTS;
        for (k, series) in self.data.series.iter().enumerate() {
            let color = self.colors[k];
            let mut runs: Vec<Vec<(f32, f32)>> = vec![Vec::new()];
            for (group, value) in series.values.iter().enumerate() {
                match value {
                    Some(value) => runs
                        .last_mut()
                        .into_iter()
                        .for_each(|run| run.push((step * (group as f32 + 0.5), frame.y(*value)))),
                    None if runs.last().is_some_and(|run| !run.is_empty()) => runs.push(Vec::new()),
                    None => {}
                }
            }
            for run in runs.into_iter().filter(|run| !run.is_empty()) {
                let line = Line::new()
                    .data(run)
                    .x(|p: &(f32, f32)| Some(p.0))
                    .y(|p: &(f32, f32)| Some(p.1))
                    .stroke(color)
                    .stroke_width(px(size::OPTICAL))
                    .curve(Curve::Linear);
                let line = match dots {
                    true => line
                        .dot()
                        .dot_size(px(DOT))
                        .dot_fill(cx.theme().background)
                        .dot_stroke(color),
                    false => line,
                };
                line.paint(bounds, window);
            }
        }
    }

    fn paint_labels(&self, frame: &Frame, step: f32, bounds: &Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let font = self.font.as_f32();
        let longest = self.labels.iter().map(|label| label.chars().count()).max().unwrap_or(0);
        let stride = label_stride(step, (longest as f32 * font * CHAR_WIDTH).min(MAX_LABEL));
        let room = (step * stride as f32 - LABEL_GAP).min(MAX_LABEL);
        let (width, color) = (bounds.size.width.as_f32(), cx.theme().muted_foreground);
        let mut end = f32::NEG_INFINITY;
        let mut texts = Vec::new();
        for group in (0..self.labels.len()).step_by(stride) {
            let text = truncate_text_to_width(&self.labels[group], self.font, room, window);
            let half = measure_text_width(&text, self.font, window) / 2.;
            let x = (step * (group as f32 + 0.5)).clamp(half, (width - half).max(half));
            if x - half < end + LABEL_GAP {
                continue;
            }
            end = x + half;
            let at = point(px(x), px(frame.bottom + TEXT_GAP * 3.));
            texts.push(Text::new(text, at, color).font_size(self.font).align(TextAlign::Center));
        }
        PlotLabel::new(texts).paint(bounds, window, cx);
    }
}

impl Plot for ResultPlot {
    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        if self.data.labels.is_empty() {
            return;
        }
        let (frame, step) = (self.frame(&bounds), self.step(&bounds));
        let theme = cx.theme();
        let (grid, axis) = (theme.chart_grid, theme.border);
        Grid::new()
            .y(self.ticks.iter().map(|tick| px(frame.y(*tick))))
            .stroke(grid)
            .dash_array(&[px(size::XS), px(size::OPTICAL)])
            .paint(&bounds, window);
        let (first, last) = (self.ticks.first().copied().unwrap_or(0.), self.ticks.last().copied().unwrap_or(0.));
        let base = frame.y(0f64.clamp(first, last.max(first)));
        let line = Bounds::new(
            bounds.origin + point(px(0.), px(base)),
            gpui::size(bounds.size.width, px(size::HAIRLINE)),
        );
        window.paint_quad(gpui::fill(line, axis));
        match self.kind {
            Kind::Bars => self.paint_bars(&frame, step, &bounds, window),
            Kind::Line => self.paint_lines(&frame, step, &bounds, window, cx),
        }
        self.paint_labels(&frame, step, &bounds, window, cx);
    }

    fn id(&self) -> Option<ElementId> {
        Some("result-chart-plot".into())
    }

    fn tooltip_state(&self, position: Point<Pixels>, bounds: Bounds<Pixels>, _: &App) -> Option<TooltipState> {
        let groups = self.data.labels.len();
        if groups == 0 || position.y.as_f32() > self.frame(&bounds).bottom {
            return None;
        }
        let step = self.step(&bounds);
        let group = nearest(position.x.as_f32(), step, groups);
        Some(TooltipState::new(group, point(px(step * (group as f32 + 0.5)), position.y), vec![]))
    }

    fn tooltip(
        &self,
        state: &TooltipState,
        cursor: Point<Pixels>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let group = state.index;
        let label = self.labels.get(group)?.clone();
        let (frame, step) = (self.frame(&bounds), self.step(&bounds));
        let cross = CrossLine::new(state.cross_line).height(frame.bottom);
        let cross = match self.kind {
            Kind::Bars => cross.band(px(step * (1. - GROUP_PAD / 2.))),
            Kind::Line => cross,
        };
        let mut tooltip = Tooltip::new(cursor, bounds.size)
            .gap(px(size::SM))
            .cross_line(cross)
            .title(label);
        if self.kind == Kind::Line {
            let background = cx.theme().background;
            let dots = self.data.series.iter().zip(self.colors.iter()).filter_map(|(series, color)| {
                let y = frame.y((*series.values.get(group)?)?);
                Some(
                    Dot::new(point(state.cross_line.x, px(y)))
                        .size(px(HOVER_DOT))
                        .stroke(*color)
                        .fill(background),
                )
            });
            tooltip = tooltip.dots(dots.collect::<Vec<_>>());
        }
        for ((series, name), color) in self.data.series.iter().zip(self.names.iter()).zip(self.colors.iter()) {
            let value = series
                .values
                .get(group)
                .copied()
                .flatten()
                .map_or_else(|| "NULL".to_string(), |value| number(value, 2));
            tooltip = tooltip.row(*color, name.clone(), value);
        }
        Some(tooltip.into_any_element())
    }
}

#[derive(IntoPlot)]
pub(crate) struct ValueAxis {
    pub ticks: Rc<[f64]>,
    pub labels: Rc<[SharedString]>,
    pub font: Pixels,
}

impl Plot for ValueAxis {
    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let font = self.font.as_f32();
        let frame = Frame::new(bounds.size.height.as_f32(), font, &self.ticks);
        let right = bounds.size.width.as_f32() - LABEL_GAP;
        let color = cx.theme().muted_foreground;
        let texts = self
            .ticks
            .iter()
            .zip(self.labels.iter())
            .map(|(tick, label)| {
                let at = point(px(right), px(frame.y(*tick) - font / 2.));
                Text::new(label.clone(), at, color).font_size(self.font).align(TextAlign::Right)
            })
            .collect();
        PlotLabel::new(texts).paint(&bounds, window, cx);
    }
}

#[cfg(test)]
#[path = "chart_plot_tests.rs"]
mod tests;
