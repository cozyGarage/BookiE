use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use bookie_app_services::result_scan::ScanSource;
use relm4::gtk;
use relm4::gtk::glib::WeakRef;
use relm4::gtk::prelude::*;
use tokio_util::sync::CancellationToken;

use super::super::stream_pages::StreamedResult;
use super::super::{SqlEditorInput, StatementIdentity};
use super::cursor::MatchCursor;
use super::jump::JumpTicket;
use super::ticket::FilterTicket;

pub(in crate::ui::editor) struct ToolsTarget<'a> {
    pub(in crate::ui::editor) holder: &'a gtk::Box,
    pub(in crate::ui::editor) header: &'a relm4::adw::WrapBox,
    pub(in crate::ui::editor) pages: &'a gtk::Stack,
    pub(in crate::ui::editor) result: &'a StreamedResult,
    pub(in crate::ui::editor) identity: StatementIdentity,
    pub(in crate::ui::editor) sender: &'a relm4::Sender<SqlEditorInput>,
}

pub(super) struct Controls {
    pub(super) entry: WeakRef<gtk::SearchEntry>,
    pub(super) status: WeakRef<gtk::Label>,
    pub(super) previous: WeakRef<gtk::Button>,
    pub(super) next: WeakRef<gtk::Button>,
    pub(super) filter: WeakRef<gtk::Button>,
    pub(super) stop: WeakRef<gtk::Button>,
}

pub(super) struct Tools {
    pub(super) source: ScanSource,
    pub(super) result: StreamedResult,
    pub(super) identity: StatementIdentity,
    pub(super) sender: relm4::Sender<SqlEditorInput>,
    pub(super) holder: WeakRef<gtk::Box>,
    pub(super) pages: WeakRef<gtk::Stack>,
    pub(super) controls: Controls,
    pub(super) cursor: RefCell<MatchCursor>,
    pub(super) cancel: RefCell<CancellationToken>,
    pub(super) filter_ticket: RefCell<Option<Arc<FilterTicket>>>,
    pub(super) jump: JumpTicket,
    pub(super) busy: Cell<bool>,
}

thread_local! {
    static TOOLS: RefCell<Vec<(StatementIdentity, Weak<Tools>)>> = const { RefCell::new(Vec::new()) };
}

fn register_tools(identity: StatementIdentity, tools: &Rc<Tools>) {
    TOOLS.with(|registry| {
        let mut registry = registry.borrow_mut();
        registry.retain(|(_, tools)| tools.strong_count() > 0);
        registry.push((identity, Rc::downgrade(tools)));
    });
}

pub(super) fn tools_for(identity: StatementIdentity) -> Option<Rc<Tools>> {
    TOOLS.with(|registry| {
        registry
            .borrow()
            .iter()
            .rev()
            .filter(|(candidate, _)| *candidate == identity)
            .find_map(|(_, tools)| tools.upgrade())
    })
}

fn labelled_button(icon: &str, name: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_widget_name(name);
    button.set_tooltip_text(Some(label));
    button.add_css_class("flat");
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    button
}

struct Widgets {
    find: gtk::Box,
    actions: gtk::Box,
    entry: gtk::SearchEntry,
    status: gtk::Label,
    previous: gtk::Button,
    next: gtk::Button,
    filter: gtk::Button,
    stop: gtk::Button,
}

fn build_widgets() -> Widgets {
    let find = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    find.set_widget_name("tp-result-tools");
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    actions.set_widget_name("tp-result-actions");
    let entry = gtk::SearchEntry::new();
    entry.set_widget_name("tp-result-find-entry");
    entry.set_width_chars(18);
    entry.set_max_width_chars(24);
    entry.set_placeholder_text(Some(&crate::tr!("Find in all rows")));
    entry.update_property(&[gtk::accessible::Property::Label(&crate::tr!("Find in all rows"))]);
    let previous = labelled_button(
        "go-up-symbolic",
        "tp-result-find-previous",
        &crate::tr!("Previous match"),
    );
    let next = labelled_button("go-down-symbolic", "tp-result-find-next", &crate::tr!("Next match"));
    let status = gtk::Label::new(None);
    status.set_widget_name("tp-result-find-status");
    status.set_xalign(0.0);
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status.set_max_width_chars(30);
    let filter = gtk::Button::with_label(&crate::tr!("Show matching rows"));
    filter.set_widget_name("tp-result-filter-button");
    filter.add_css_class("flat");
    let stop = gtk::Button::with_label(&crate::tr!("Cancel"));
    stop.set_widget_name("tp-result-find-stop");
    stop.set_visible(false);
    for widget in [
        entry.upcast_ref::<gtk::Widget>(),
        previous.upcast_ref(),
        next.upcast_ref(),
        status.upcast_ref(),
    ] {
        find.append(widget);
    }
    actions.append(&filter);
    actions.append(&stop);
    Widgets {
        find,
        actions,
        entry,
        status,
        previous,
        next,
        filter,
        stop,
    }
}

pub(in crate::ui::editor) fn append(target: ToolsTarget<'_>) {
    let ToolsTarget {
        holder,
        header,
        pages,
        result,
        identity,
        sender,
    } = target;
    let Some(spool) = result.spool.clone() else { return };
    let widgets = build_widgets();
    let tools = Rc::new(Tools {
        source: ScanSource::new(spool, result.columns.clone(), result.total_rows as u64),
        result: result.clone(),
        identity,
        sender: sender.clone(),
        holder: holder.downgrade(),
        pages: pages.downgrade(),
        controls: Controls {
            entry: widgets.entry.downgrade(),
            status: widgets.status.downgrade(),
            previous: widgets.previous.downgrade(),
            next: widgets.next.downgrade(),
            filter: widgets.filter.downgrade(),
            stop: widgets.stop.downgrade(),
        },
        cursor: RefCell::new(MatchCursor::default()),
        cancel: RefCell::new(CancellationToken::new()),
        filter_ticket: RefCell::new(None),
        jump: JumpTicket::default(),
        busy: Cell::new(false),
    });
    super::actions::connect(
        &tools,
        &widgets.find,
        &widgets.entry,
        &widgets.previous,
        &widgets.next,
        &widgets.filter,
        &widgets.stop,
    );
    header.append(&widgets.find);
    header.append(&widgets.actions);
    register_tools(identity, &tools);
    if let Some(ticket) = super::ticket::active(identity) {
        tools.adopt_filter(ticket);
    }
}
