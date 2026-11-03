use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use relm4::gtk;
use relm4::gtk::prelude::*;

use super::super::stream_view::page_widget_name;

const RETRY_INTERVAL: Duration = Duration::from_millis(50);
const MAX_RETRIES: u32 = 120;

pub(in crate::ui::editor) fn continuous_container_name(generation: u64) -> String {
    format!("tp-continuous-results-{generation}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct JumpTarget {
    pub(super) generation: u64,
    pub(super) page: usize,
    pub(super) row: usize,
    pub(super) global_row: usize,
    pub(super) page_rows: usize,
}

pub(super) fn find_descendant<T: IsA<gtk::Widget>>(
    root: &gtk::Widget,
    matches: impl Fn(&gtk::Widget) -> bool,
) -> Option<T> {
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        if matches(&widget)
            && let Ok(found) = widget.clone().downcast::<T>()
        {
            return Some(found);
        }
        let mut child = widget.first_child();
        while let Some(next) = child {
            child = next.next_sibling();
            pending.push(next);
        }
    }
    None
}

fn named_descendant<T: IsA<gtk::Widget>>(root: &gtk::Widget, name: &str) -> Option<T> {
    find_descendant(root, |widget| widget.widget_name() == name)
}

fn select_row(view: &gtk::ColumnView, row: usize) {
    view.scroll_to(
        row as u32,
        None,
        gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT,
        None,
    );
}

pub(super) fn position_of_source(model: &gtk::gio::ListModel, source: usize) -> Option<u32> {
    (0..model.n_items()).find(|position| {
        model
            .item(*position)
            .and_downcast::<crate::ui::row_object::RowObject>()
            .and_then(|row| row.source_index())
            == Some(source)
    })
}

fn continuous_view(holder: &gtk::Box, generation: u64) -> Option<gtk::ColumnView> {
    let root = holder.clone().upcast::<gtk::Widget>();
    let container = named_descendant::<gtk::Box>(&root, &continuous_container_name(generation))?;
    if !container.is_visible() {
        return None;
    }
    find_descendant(container.upcast_ref(), |widget| widget.is::<gtk::ColumnView>())
}

fn page_view(holder: &gtk::Box, target: &JumpTarget) -> Option<gtk::ColumnView> {
    let root = holder.clone().upcast::<gtk::Widget>();
    let name = page_widget_name(target.generation, target.page as u64);
    let page = named_descendant::<gtk::Box>(&root, &name)?;
    find_descendant(page.upcast_ref(), |widget| widget.is::<gtk::ColumnView>())
}

#[derive(Default)]
pub(super) struct JumpTicket(Rc<Cell<u64>>);

impl JumpTicket {
    fn next(&self) -> u64 {
        self.0.set(self.0.get().wrapping_add(1));
        self.0.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Attempt {
    Select,
    Retry,
    GiveUp,
}

pub(super) fn attempt(view_rows: Option<usize>, expected_rows: Option<usize>, tries: u32) -> Attempt {
    match view_rows {
        Some(rows) if rows > 0 && expected_rows.is_none_or(|expected| expected == rows) => Attempt::Select,
        Some(rows) if rows > 0 => Attempt::GiveUp,
        _ if tries > MAX_RETRIES => Attempt::GiveUp,
        _ => Attempt::Retry,
    }
}

pub(super) fn resolve_position(continuous: bool, row: usize, source_position: Option<u32>) -> Option<usize> {
    if continuous {
        return Some(row);
    }
    source_position.map(|position| position as usize)
}

fn view_rows(view: Option<&gtk::ColumnView>) -> Option<usize> {
    view.map(|view| view.model().map_or(0, |model| model.n_items() as usize))
}

pub(super) fn go_to(
    holder: &gtk::Box,
    pages: &gtk::Stack,
    target: JumpTarget,
    ticket: &JumpTicket,
    failed: Rc<dyn Fn()>,
) {
    let id = ticket.next();
    let continuous = continuous_visible(holder, target.generation);
    if !continuous {
        pages.set_visible_child_name(&page_widget_name(target.generation, target.page as u64));
    }
    let holder = holder.downgrade();
    let current = ticket.0.clone();
    let tries = Cell::new(0_u32);
    gtk::glib::timeout_add_local(RETRY_INTERVAL, move || {
        let Some(holder) = holder.upgrade() else {
            return gtk::glib::ControlFlow::Break;
        };
        tries.set(tries.get() + 1);
        if current.get() != id {
            return gtk::glib::ControlFlow::Break;
        }
        let (view, expected, row) = if continuous {
            (continuous_view(&holder, target.generation), None, target.global_row)
        } else {
            (page_view(&holder, &target), Some(target.page_rows), target.row)
        };
        let rows = view_rows(view.as_ref());
        match (attempt(rows, expected, tries.get()), view) {
            (Attempt::Select, Some(view)) => {
                let source_position = (!continuous)
                    .then(|| {
                        view.model()
                            .and_then(|model| position_of_source(model.upcast_ref(), row))
                    })
                    .flatten();
                match resolve_position(continuous, row, source_position) {
                    Some(position) => select_row(&view, position),
                    None => failed(),
                }
                gtk::glib::ControlFlow::Break
            }
            (Attempt::Retry, _) => gtk::glib::ControlFlow::Continue,
            _ => {
                failed();
                gtk::glib::ControlFlow::Break
            }
        }
    });
}

fn continuous_visible(holder: &gtk::Box, generation: u64) -> bool {
    let root = holder.clone().upcast::<gtk::Widget>();
    named_descendant::<gtk::Box>(&root, &continuous_container_name(generation))
        .is_some_and(|container| container.is_visible())
}
