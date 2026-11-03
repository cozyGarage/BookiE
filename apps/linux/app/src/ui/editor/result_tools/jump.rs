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

pub(super) fn go_to(holder: &gtk::Box, pages: &gtk::Stack, target: JumpTarget, ticket: &JumpTicket) {
    let id = ticket.next();
    if let Some(view) = continuous_view(holder, target.generation) {
        select_row(&view, target.global_row);
        return;
    }
    pages.set_visible_child_name(&page_widget_name(target.generation, target.page as u64));
    let holder = holder.downgrade();
    let current = ticket.0.clone();
    let tries = Cell::new(0_u32);
    gtk::glib::timeout_add_local(RETRY_INTERVAL, move || {
        let Some(holder) = holder.upgrade() else {
            return gtk::glib::ControlFlow::Break;
        };
        tries.set(tries.get() + 1);
        if current.get() != id || tries.get() > MAX_RETRIES {
            return gtk::glib::ControlFlow::Break;
        }
        let Some(view) = page_view(&holder, &target) else {
            return gtk::glib::ControlFlow::Continue;
        };
        let rows = view.model().map_or(0, |model| model.n_items() as usize);
        if rows == 0 {
            return gtk::glib::ControlFlow::Continue;
        }
        if rows == target.page_rows {
            select_row(&view, target.row);
        }
        gtk::glib::ControlFlow::Break
    });
}
