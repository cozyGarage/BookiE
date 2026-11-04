use std::ops::Range;
use std::time::{Duration, Instant};

use bookie_gpui_grid_prototype::model::*;
use bookie_gpui_grid_prototype::selection::{Selection, copy_tsv};
use gpui::{
    App, Application, Bounds, ClipboardItem, Context, Div, FocusHandle, Focusable, KeyDownEvent, MouseButton,
    MouseDownEvent, ScrollStrategy, Stateful, UniformListScrollHandle, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size, uniform_list,
};

const ROW_HEIGHT: f32 = 36.0;
const CELL_WIDTHS: [f32; 4] = [240.0, 280.0, 120.0, 320.0];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    None,
    Find,
    Filter,
}

enum Mode {
    All,
    Filtered { kept: Vec<u32>, label: String },
}

struct GridView {
    total: u64,
    mode: Mode,
    pager: Pager,
    continuous: bool,
    find_text: String,
    filter_text: String,
    filter_visible: bool,
    active: Field,
    cursor: MatchCursor,
    find_capped: bool,
    compare: Option<Diff>,
    status: String,
    selection: Option<Selection>,
    generation: u64,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    started: Instant,
    first_frame_reported: bool,
    last_frame: Option<Instant>,
    frame_intervals: Vec<Duration>,
    measuring: bool,
    pace: Option<Pace>,
}

struct Pace {
    step: u32,
    steps: u32,
    begun: Instant,
}

fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn env_number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|value| value.parse().ok())
}

impl Focusable for GridView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl GridView {
    fn new(total: u64, started: Instant, cx: &mut Context<Self>) -> Self {
        GridView {
            total,
            mode: Mode::All,
            pager: Pager::new(total),
            continuous: false,
            find_text: String::new(),
            filter_text: String::new(),
            filter_visible: false,
            active: Field::None,
            cursor: MatchCursor::default(),
            find_capped: false,
            compare: None,
            status: String::new(),
            selection: None,
            generation: 0,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            started,
            first_frame_reported: false,
            last_frame: None,
            frame_intervals: Vec::new(),
            measuring: false,
            pace: None,
        }
    }

    fn dataset_count(&self) -> u64 {
        match &self.mode {
            Mode::All => self.total,
            Mode::Filtered { kept, .. } => kept.len() as u64,
        }
    }

    fn base(&self) -> u64 {
        if self.continuous {
            0
        } else {
            self.pager.page * PAGE_ROWS
        }
    }

    fn visible_count(&self) -> usize {
        if let Some(diff) = &self.compare {
            return diff.rows.len();
        }
        let remaining = self.dataset_count().saturating_sub(self.base());
        if self.continuous {
            remaining as usize
        } else {
            remaining.min(PAGE_ROWS) as usize
        }
    }

    fn source_row(&self, dataset_index: u64) -> u64 {
        match &self.mode {
            Mode::All => dataset_index,
            Mode::Filtered { kept, .. } => u64::from(kept[dataset_index as usize]),
        }
    }

    fn dataset_index_of(&self, source: u64) -> Option<u64> {
        match &self.mode {
            Mode::All => Some(source),
            Mode::Filtered { kept, .. } => kept.binary_search(&(source as u32)).ok().map(|index| index as u64),
        }
    }

    fn click_row(&mut self, dataset_index: u64, shift: bool) {
        self.selection = Some(Selection::after_click(self.selection, dataset_index, shift));
        self.status = self.selection.map(Selection::label).unwrap_or_default();
    }

    fn copy_selection(&mut self, with_headers: bool, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else { return };
        if !selection.copyable() {
            self.status = selection.label();
            return;
        }
        let rows = (selection.first()..=selection.last()).map(|index| row_at(self.source_row(index)));
        let text = copy_tsv(rows, with_headers);
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        let noun = if selection.count() == 1 { "row" } else { "rows" };
        self.status = format!(
            "Copied {} {noun}{}",
            selection.count(),
            if with_headers { " with headers" } else { "" }
        );
    }

    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.pager = Pager::new(self.dataset_count());
        self.selection = None;
        self.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
    }

    fn note_frame(&mut self) {
        let now = Instant::now();
        if !self.first_frame_reported {
            self.first_frame_reported = true;
            println!("PROTOTYPE_FIRST_FRAME ms={}", self.started.elapsed().as_millis());
        }
        if let (true, Some(last)) = (self.measuring, self.last_frame) {
            self.frame_intervals.push(now - last);
        }
        self.last_frame = Some(now);
    }

    fn begin_background<T: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce() -> T + Send + 'static,
        apply: impl FnOnce(&mut Self, T) + 'static,
    ) {
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { work() }).await;
            this.update(cx, |this, cx| {
                if this.generation == generation {
                    apply(this, result);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn run_find(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.find_text.is_empty() {
            return;
        }
        if !self.cursor.matches.is_empty() && self.cursor.status(false) != "No match" {
            self.step_match(forward);
            return;
        }
        let (total, query) = (self.total, self.find_text.clone());
        self.status = "Searching…".into();
        self.begin_background(
            cx,
            move || find_rows(total, &query, MAX_FIND_MATCHES),
            move |this, found| match found {
                Ok(found) => {
                    this.find_capped = found.capped;
                    this.cursor.set(found.matches);
                    this.step_match(forward);
                }
                Err(message) => this.status = message.into(),
            },
        );
    }

    fn step_match(&mut self, forward: bool) {
        if let Some(row) = self.cursor.step(forward) {
            let at = self.dataset_index_of(row);
            self.selection = at.map(|index| Selection::after_click(None, index, false));
            if !self.continuous {
                self.pager.contains_row(row);
            }
            let local = row.saturating_sub(self.base()) as usize;
            self.scroll.scroll_to_item_strict(local, ScrollStrategy::Center);
        }
        self.status = self.cursor.status(self.find_capped);
    }

    fn show_matching(&mut self) {
        if self.cursor.matches.is_empty() {
            return;
        }
        let kept = self.cursor.matches.iter().map(|row| *row as u32).collect();
        let label = format!("FILTER {}", self.find_text);
        self.set_mode(Mode::Filtered { kept, label });
        self.status = format!("Showing {} matching rows", thousands(self.dataset_count()));
    }

    fn apply_filter(&mut self, cx: &mut Context<Self>) {
        let predicate = match parse_predicate(&self.filter_text) {
            Ok(predicate) => predicate,
            Err(message) => {
                self.status = message.into();
                return;
            }
        };
        let total = self.total;
        let label = format!("WHERE {}", self.filter_text);
        self.status = "Filtering…".into();
        self.begin_background(
            cx,
            move || filter_rows(total, &predicate),
            move |this, kept| {
                this.set_mode(Mode::Filtered { kept, label });
                this.status = format!("Showing {} matching rows", thousands(this.dataset_count()));
            },
        );
    }

    fn clear_filter(&mut self) {
        self.generation += 1;
        self.cursor = MatchCursor::default();
        self.set_mode(Mode::All);
        self.status.clear();
    }

    fn compare(&mut self, keyed: bool, cx: &mut Context<Self>) {
        let total = self.total;
        self.status = "Comparing…".into();
        self.begin_background(
            cx,
            move || build_diff(total, keyed),
            |this, diff| {
                this.status.clear();
                this.compare = Some(diff);
                this.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
            },
        );
    }

    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let stroke = &event.keystroke;
        if stroke.modifiers.control && stroke.key == "f" {
            self.filter_visible = true;
            self.active = Field::Filter;
            cx.notify();
            return;
        }
        if stroke.modifiers.control && stroke.key == "c" && self.active == Field::None {
            self.copy_selection(stroke.modifiers.shift, cx);
            cx.notify();
            return;
        }
        let field = match self.active {
            Field::Find => &mut self.find_text,
            Field::Filter => &mut self.filter_text,
            Field::None => return,
        };
        match stroke.key.as_str() {
            "backspace" => {
                field.pop();
                if self.active == Field::Find {
                    self.cursor = MatchCursor::default();
                }
            }
            "escape" => self.active = Field::None,
            "enter" if self.active == Field::Find => self.run_find(!stroke.modifiers.shift, cx),
            "enter" => self.apply_filter(cx),
            _ => {
                if let (Some(text), false) = (&stroke.key_char, stroke.modifiers.control) {
                    field.push_str(text);
                    if self.active == Field::Find {
                        self.cursor = MatchCursor::default();
                    }
                }
            }
        }
        cx.notify();
    }

    fn pace_frame(&mut self, window: &mut Window) {
        let last = self.visible_count().saturating_sub(1);
        let Some(pace) = self.pace.as_mut() else { return };
        let target = (f64::from(pace.step) / f64::from(pace.steps) * last as f64) as usize;
        self.scroll.scroll_to_item_strict(target, ScrollStrategy::Top);
        pace.step += 1;
        if pace.step <= pace.steps {
            window.request_animation_frame();
            return;
        }
        let elapsed = pace.begun.elapsed().as_millis();
        let steps = pace.steps;
        self.pace = None;
        self.measuring = false;
        self.report_scroll(steps, elapsed, true);
    }

    fn report_scroll(&self, steps: u32, elapsed: u128, with_frames: bool) {
        let mut sorted: Vec<f64> = self.frame_intervals.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
        sorted.sort_by(f64::total_cmp);
        let pick = |q: f64| {
            sorted
                .get(((sorted.len() as f64 - 1.0) * q) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        let frames = if with_frames {
            format!(
                " frames={} frame_p50_ms={:.2} frame_p95_ms={:.2} frame_p99_ms={:.2} frame_max_ms={:.2}",
                sorted.len(),
                pick(0.5),
                pick(0.95),
                pick(0.99),
                pick(1.0)
            )
        } else {
            String::new()
        };
        println!("PROTOTYPE_SCROLL_DONE steps={steps} elapsed_ms={elapsed}{frames}");
    }

    fn start_autoscroll(&mut self, steps: u32, cx: &mut Context<Self>) {
        if std::env::var("BOOKIE_PROTOTYPE_AUTOSCROLL_SCOPE").as_deref() == Ok("continuous") {
            self.continuous = true;
        }
        let frame_paced = std::env::var("BOOKIE_PROTOTYPE_AUTOSCROLL_PACING").as_deref() == Ok("frame");
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            if frame_paced {
                this.update(cx, |this, cx| {
                    this.frame_intervals.clear();
                    this.last_frame = None;
                    this.measuring = true;
                    this.pace = Some(Pace {
                        step: 0,
                        steps,
                        begun: Instant::now(),
                    });
                    cx.notify();
                })
                .ok();
                return;
            }
            let begun = Instant::now();
            for step in 0..=steps {
                let moved = this.update(cx, |this, cx| {
                    let last = this.visible_count().saturating_sub(1);
                    let target = (f64::from(step) / f64::from(steps) * last as f64) as usize;
                    this.scroll.scroll_to_item_strict(target, ScrollStrategy::Top);
                    cx.notify();
                });
                if moved.is_err() {
                    return;
                }
                cx.background_executor().timer(Duration::from_millis(62)).await;
            }
            let elapsed = begun.elapsed().as_millis();
            this.update(cx, |this, _| this.report_scroll(steps, elapsed, false))
                .ok();
        })
        .detach();
    }
}

fn button(id: &'static str, label: impl Into<String>) -> Stateful<Div> {
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_md()
        .bg(rgb(0x2d2f36))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x3a3d46)))
        .child(label.into())
}

fn field_box(id: &'static str, text: &str, placeholder: &str, active: bool) -> Stateful<Div> {
    let shown = if text.is_empty() {
        placeholder.to_string()
    } else {
        text.to_string()
    };
    div()
        .id(id)
        .w(px(220.0))
        .px_2()
        .py_1()
        .border_1()
        .border_color(if active { rgb(0x4c8dff) } else { rgb(0x44464f) })
        .bg(rgb(0x14151a))
        .text_color(if text.is_empty() { rgb(0x80838f) } else { rgb(0xe6e6e6) })
        .cursor_text()
        .child(shown)
}

fn cell_div(width: f32, text: String) -> Div {
    div().w(px(width)).flex_none().px_2().overflow_hidden().child(text)
}

fn row_div(stripe: bool, selected: bool) -> Div {
    let background = if selected {
        rgb(0x2a4a7f)
    } else if stripe {
        rgb(0x202228)
    } else {
        rgb(0x1b1c20)
    };
    div().flex().items_center().h(px(ROW_HEIGHT)).w_full().bg(background)
}

impl Render for GridView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.note_frame();
        self.pace_frame(window);
        let (first, last) = self.pager.range();
        let title = match &self.mode {
            Mode::Filtered { label, .. } => format!("{label} · "),
            Mode::All => String::new(),
        };
        let progress = format!(
            "{title}Complete · Received {} rows · {} pages",
            thousands(self.dataset_count()),
            self.pager.pages()
        );
        let range_label = if self.dataset_count() == 0 {
            "Rows 0".to_string()
        } else if self.continuous {
            format!("Rows 1-{}", thousands(self.dataset_count()))
        } else {
            format!("Rows {}-{}", thousands(first), thousands(last))
        };
        let strip = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .p_2()
            .child(div().child(progress))
            .child(button("first", "First").on_click(cx.listener(|this, _, _, cx| {
                this.pager.first();
                cx.notify();
            })))
            .child(button("prev", "Previous page").on_click(cx.listener(|this, _, _, cx| {
                this.pager.previous();
                cx.notify();
            })))
            .child(div().child(range_label))
            .child(button("next", "Next page").on_click(cx.listener(|this, _, _, cx| {
                this.pager.next();
                cx.notify();
            })))
            .child(button("last", "Last").on_click(cx.listener(|this, _, _, cx| {
                this.pager.last();
                cx.notify();
            })))
            .child(
                field_box("find", &self.find_text, "Find in all rows", self.active == Field::Find).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.active = Field::Find;
                        cx.notify();
                    }),
                ),
            )
            .child(
                button("find-prev", "Previous match").on_click(cx.listener(|this, _, _, cx| this.run_find(false, cx))),
            )
            .child(button("find-next", "Next match").on_click(cx.listener(|this, _, _, cx| this.run_find(true, cx))))
            .child(
                button("show", "Show matching rows").on_click(cx.listener(|this, _, _, cx| {
                    this.show_matching();
                    cx.notify();
                })),
            )
            .child(button("compare", "Compare…").on_click(cx.listener(|this, _, _, cx| this.compare(false, cx))))
            .child(button("export", "Export all rows…"))
            .child(
                button(
                    "continuous",
                    if self.continuous {
                        "Continuous scrolling: on"
                    } else {
                        "Continuous scrolling"
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.continuous = !this.continuous;
                    this.scroll.scroll_to_item_strict(0, ScrollStrategy::Top);
                    cx.notify();
                })),
            )
            .child(div().text_color(rgb(0x9aa0b4)).child(self.status.clone()));
        let filter_row = self.filter_visible.then(|| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .pb_2()
                .child(div().child("WHERE"))
                .child(
                    field_box(
                        "where",
                        &self.filter_text,
                        "label contains 7",
                        self.active == Field::Filter,
                    )
                    .w(px(420.0))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.active = Field::Filter;
                        cx.notify();
                    })),
                )
                .child(button("apply", "Apply").on_click(cx.listener(|this, _, _, cx| this.apply_filter(cx))))
                .child(button("clear", "Clear all").on_click(cx.listener(|this, _, _, cx| {
                    this.clear_filter();
                    cx.notify();
                })))
        });
        let compare_bar = self.compare.as_ref().map(|diff| {
            let s = &diff.summary;
            let summary = if s.keyed {
                format!(
                    "Equal {} · Removed {} · Changed {}",
                    thousands(s.equal),
                    thousands(s.removed),
                    thousands(s.changed)
                )
            } else {
                format!(
                    "Equal {} · Added {} · Removed {}",
                    thousands(s.equal),
                    thousands(s.added),
                    thousands(s.removed)
                )
            };
            let capped = if diff.capped {
                format!(" · first {MAX_DIFF_ROWS} differences shown")
            } else {
                String::new()
            };
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .pb_2()
                .child(div().child(format!("COMPARE {summary}{capped}")))
                .child(button("whole", "Whole row").on_click(cx.listener(|this, _, _, cx| this.compare(false, cx))))
                .child(button("keyed", "Key: value").on_click(cx.listener(|this, _, _, cx| this.compare(true, cx))))
                .child(button("close-compare", "Close").on_click(cx.listener(|this, _, _, cx| {
                    this.compare = None;
                    cx.notify();
                })))
        });
        let header = row_div(false, false)
            .text_color(rgb(0x9aa0b4))
            .child(cell_div(CELL_WIDTHS[0], "value".into()))
            .child(cell_div(CELL_WIDTHS[1], "label".into()))
            .child(cell_div(CELL_WIDTHS[2], "state".into()))
            .child(cell_div(CELL_WIDTHS[3], "typed".into()));
        let list = uniform_list(
            "rows",
            self.visible_count(),
            cx.processor(|this, range: Range<usize>, _window, cx| {
                range
                    .map(|index| this.row_at_position(index, cx))
                    .collect::<Vec<Stateful<Div>>>()
            }),
        )
        .track_scroll(self.scroll.clone())
        .flex_1();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1b1c20))
            .text_color(rgb(0xe6e6e6))
            .text_sm()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .child(strip)
            .children(filter_row)
            .children(compare_bar)
            .child(header)
            .child(list)
    }
}

impl GridView {
    fn row_at_position(&self, position: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        if let Some(diff) = &self.compare {
            let Some(row) = diff.rows.get(position) else {
                return row_div(false, false).id(position);
            };
            return row_div(position % 2 == 1, false)
                .id(position)
                .child(cell_div(70.0, row.change.to_string()))
                .child(cell_div(CELL_WIDTHS[0], row.cells[0].display()))
                .child(cell_div(CELL_WIDTHS[1], row.cells[1].display()))
                .child(cell_div(CELL_WIDTHS[2], row.cells[2].display()))
                .child(cell_div(CELL_WIDTHS[3], row.cells[3].display()));
        }
        let dataset_index = self.base() + position as u64;
        let source = self.source_row(dataset_index);
        let cells = row_at(source);
        let selected = self
            .selection
            .is_some_and(|selection| selection.contains(dataset_index));
        row_div(position % 2 == 1, selected)
            .id(position)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.click_row(dataset_index, event.modifiers.shift);
                    cx.notify();
                }),
            )
            .child(cell_div(CELL_WIDTHS[0], cells[0].display()))
            .child(cell_div(CELL_WIDTHS[1], cells[1].display()))
            .child(cell_div(CELL_WIDTHS[2], cells[2].display()))
            .child(cell_div(CELL_WIDTHS[3], cells[3].display()))
    }
}

fn main() {
    let started = Instant::now();
    let total = rows_from(std::env::var("BOOKIE_PROTOTYPE_ROWS").ok().as_deref());
    let autoscroll: u32 = env_number("BOOKIE_PROTOTYPE_AUTOSCROLL").unwrap_or(0);
    let width: f32 = env_number("BOOKIE_PROTOTYPE_WIDTH").unwrap_or(1100.0);
    let height: f32 = env_number("BOOKIE_PROTOTYPE_HEIGHT").unwrap_or(800.0);
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| GridView::new(total, started, cx));
                view.read(cx).focus.focus(window);
                if autoscroll > 0 {
                    view.update(cx, |view, cx| view.start_autoscroll(autoscroll, cx));
                }
                view
            },
        );
        if let Err(error) = opened {
            eprintln!("could not open a window: {error}");
            std::process::exit(2);
        }
    });
}
