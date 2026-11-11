mod continuous;
mod paged_model;
pub(super) use continuous::install as install_continuous;
mod header;
pub(super) use header::build as build_header;
mod pager;
pub(super) use pager::build as build_pager;

use bookie_core::QueryResult;
use relm4::gtk;
use relm4::gtk::prelude::*;
use std::sync::Arc;

use super::StatementOutcome;
use super::outcomes::stream_result_view::{
    build_stream_page_widget, replace_stream_page_content, stream_page_container, stream_page_label,
    stream_page_placeholder, stream_page_placeholder_with_title, stream_progress_text,
};
use super::stream_pages::StreamedResult;
use crate::ui::grid::GridMsg;
use crate::ui::grid::local_filter::LocalResultSettingsStore;

pub(super) const LIVE_PAGES: &str = "tp-live-stream-pages";
pub(super) const LIVE_PROGRESS: &str = "tp-live-stream-progress";
const PAGE_PREFIX: &str = "tp-stream-page-";
const COMPLETED_PAGES_PREFIX: &str = "tp-completed-stream-pages-";

#[derive(Default)]
pub(super) struct EvictionCursor {
    generation: Option<u64>,
    next_index: u64,
    next_page: Option<gtk::glib::WeakRef<gtk::Widget>>,
}

impl EvictionCursor {
    fn prepare(&mut self, pages: &gtk::Stack, generation: u64) {
        let current = self.next_page.as_ref().and_then(gtk::glib::WeakRef::upgrade);
        let is_current_page = current
            .as_ref()
            .is_some_and(|page| page.parent().as_ref() == Some(pages.upcast_ref::<gtk::Widget>()));
        if self.generation == Some(generation) && is_current_page {
            return;
        }
        self.generation = Some(generation);
        self.next_index = 0;
        self.next_page = pages.first_child().map(|page| page.downgrade());
    }
}

pub(super) fn page_widget_name(generation: u64, page_index: u64) -> String {
    format!("{PAGE_PREFIX}{generation}-{page_index}")
}

pub(super) fn completed_page_stack_name(generation: u64) -> String {
    format!("{COMPLETED_PAGES_PREFIX}{generation}")
}

pub(super) fn visible_page_index(holder: &gtk::Box, generation: u64) -> Option<u64> {
    let root = holder.clone().upcast::<gtk::Widget>();
    let pages = named::<gtk::Stack>(&root, LIVE_PAGES)?;
    let visible = pages.visible_child()?;
    page_index_from_name(&visible, generation)
}

pub(super) fn restore_page_selection(holder: &gtk::Box, generation: u64, page_index: u64) {
    let root = holder.clone().upcast::<gtk::Widget>();
    let Some(pages) = named::<gtk::Stack>(&root, &completed_page_stack_name(generation)) else {
        return;
    };
    let Some(page) = pages.child_by_name(&page_widget_name(generation, page_index)) else {
        return;
    };
    pages.set_visible_child(&page);
}

#[cfg(test)]
thread_local! { static NAMED_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
thread_local! { static EVICTION_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

fn named<T: IsA<gtk::Widget>>(root: &gtk::Widget, name: &str) -> Option<T> {
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        #[cfg(test)]
        NAMED_VISITS.with(|count| count.set(count.get() + 1));
        if widget.widget_name() == name
            && let Ok(found) = widget.clone().downcast::<T>()
        {
            return Some(found);
        }
        if name == LIVE_PROGRESS && widget.widget_name() == LIVE_PAGES && widget.is::<gtk::Stack>() {
            continue;
        }
        let mut child = widget.first_child();
        while let Some(next) = child {
            child = next.next_sibling();
            pending.push(next);
        }
    }
    None
}

pub(super) struct LiveStreamTarget<'a> {
    pub(super) grid_sender: &'a relm4::Sender<GridMsg>,
    pub(super) connection_id: Option<uuid::Uuid>,
    pub(super) database: std::sync::Arc<crate::services::database_service::DatabaseService>,
    pub(super) settings_store: LocalResultSettingsStore,
}

pub(super) fn update_live_stream(
    holder: &gtk::Box,
    outcome: &StatementOutcome,
    result: &StreamedResult,
    target: &LiveStreamTarget<'_>,
    cursor: &mut EvictionCursor,
) -> bool {
    let root = holder.clone().upcast::<gtk::Widget>();
    let (Some(pages), Some(progress)) = (
        named::<gtk::Stack>(&root, LIVE_PAGES),
        named::<gtk::Label>(&root, LIVE_PROGRESS),
    ) else {
        return false;
    };
    let first_resident = result.page_indices.first().copied().unwrap_or(result.total_pages);
    #[cfg(test)]
    EVICTION_VISITS.with(|count| count.set(0));
    cursor.prepare(&pages, result.generation);
    while cursor.next_index < first_resident {
        let Some(page) = cursor.next_page.as_ref().and_then(gtk::glib::WeakRef::upgrade) else {
            break;
        };
        #[cfg(test)]
        EVICTION_VISITS.with(|count| count.set(count.get() + 1));
        let next_page = page.next_sibling().map(|next| next.downgrade());
        let Some(index) = page
            .widget_name()
            .strip_prefix(PAGE_PREFIX)
            .and_then(|name| name.rsplit_once('-'))
            .and_then(|(_, index)| index.parse::<u64>().ok())
        else {
            break;
        };
        if index >= first_resident {
            break;
        }
        cursor.next_index = index.saturating_add(1);
        cursor.next_page = next_page;
        if pages.visible_child().as_ref() == Some(&page) {
            page.remove_css_class("tp-stream-page-ram-resident");
            continue;
        }
        if let Some(current) = page.first_child()
            && !current.has_css_class("tp-stream-page-placeholder")
            && !current.has_css_class("tp-stream-page-loading")
        {
            let placeholder = stream_page_placeholder(result, index);
            if let Ok(container) = page.clone().downcast::<gtk::Box>() {
                container.remove_css_class("tp-stream-page-ram-resident");
                replace_stream_page_content(&container, &placeholder);
            }
        }
    }
    let mut page_index = pages.pages().n_items() as u64;
    while page_index < result.total_pages {
        let container = stream_page_container(result, outcome.identity, page_index, target);
        pages.add_titled(
            &container,
            Some(container.widget_name().as_str()),
            &stream_page_label(result, page_index as usize),
        );
        page_index += 1;
    }
    pages.notify("visible-child");
    progress.set_text(&stream_progress_text(result));
    true
}

pub(super) fn install_page_loader(
    pages: &gtk::Stack,
    result: &StreamedResult,
    identity: super::StatementIdentity,
    sender: &relm4::Sender<super::SqlEditorInput>,
) {
    let spool = result.spool.clone();
    let resident_pages = result.resident_pages.clone();
    let columns = result.columns.clone();
    let generation = result.generation;
    let sender = sender.clone();
    let previous_index = std::rc::Rc::new(std::cell::Cell::new(
        pages
            .visible_child()
            .and_then(|child| page_index_from_name(&child, generation)),
    ));
    let initial_context = VisiblePageLoadContext {
        generation,
        identity,
        resident_pages: &resident_pages,
        spool: spool.as_ref(),
        columns: &columns,
        sender: &sender,
    };
    handle_visible_child_change(pages, &initial_context, &previous_index);
    pages.connect_visible_child_notify(move |pages| {
        let context = VisiblePageLoadContext {
            generation,
            identity,
            resident_pages: &resident_pages,
            spool: spool.as_ref(),
            columns: &columns,
            sender: &sender,
        };
        handle_visible_child_change(pages, &context, &previous_index);
    });
    pages.connect_map(|pages| pages.notify("visible-child"));
}

struct VisiblePageLoadContext<'a> {
    generation: u64,
    identity: super::StatementIdentity,
    resident_pages: &'a super::stream_pages::ResidentPageStore,
    spool: Option<&'a bookie_app_services::stream_result_spool::StreamResultSpool>,
    columns: &'a [bookie_core::ColumnInfo],
    sender: &'a relm4::Sender<super::SqlEditorInput>,
}

fn handle_visible_child_change(
    pages: &gtk::Stack,
    context: &VisiblePageLoadContext<'_>,
    previous_index: &std::cell::Cell<Option<u64>>,
) {
    let Some(container) = pages.visible_child() else {
        return;
    };
    let current_index = page_index_from_name(&container, context.generation);
    retire_previous_page(
        pages,
        context.generation,
        previous_index.replace(current_index),
        current_index,
    );
    load_visible_placeholder(pages, context);
}

fn load_visible_placeholder(pages: &gtk::Stack, context: &VisiblePageLoadContext<'_>) {
    let Some((placeholder, index)) = visible_page_placeholder(pages, context) else {
        return;
    };
    placeholder.add_css_class("tp-stream-page-loading");
    if let Some(status) = placeholder.downcast_ref::<relm4::adw::StatusPage>() {
        status.set_child(gtk::Widget::NONE);
    }
    if restore_resident_page(context, &placeholder, index) {
        return;
    }
    let Some(spool) = context.spool.cloned() else {
        if let Some(status) = placeholder.downcast_ref::<relm4::adw::StatusPage>() {
            status.set_title(&bookie_app_services::tr!("This result page is no longer available"));
        }
        placeholder.remove_css_class("tp-stream-page-loading");
        return;
    };
    request_spooled_page(context, &placeholder, index, spool);
}

fn visible_page_placeholder(pages: &gtk::Stack, context: &VisiblePageLoadContext<'_>) -> Option<(gtk::Widget, u64)> {
    let container = pages.visible_child()?;
    let placeholder = container.first_child()?;
    if !placeholder.has_css_class("tp-stream-page-placeholder") || placeholder.has_css_class("tp-stream-page-loading") {
        return None;
    }
    let index = page_index_from_name(&container, context.generation)?;
    Some((placeholder, index))
}

fn restore_resident_page(context: &VisiblePageLoadContext<'_>, placeholder: &gtk::Widget, index: u64) -> bool {
    if let Some((page, lease)) = context.resident_pages.get_with_lease(index) {
        if let Some(status) = placeholder.downcast_ref::<relm4::adw::StatusPage>() {
            status.set_description(Some(&bookie_app_services::tr!(
                "Restoring this result page from memory…"
            )));
        }
        let _ = context
            .sender
            .send(super::SqlEditorInput::Stream(super::streaming::Input::PageLoaded {
                generation: context.generation,
                index,
                identity: context.identity,
                result: Ok(page),
                resident_pages: context.resident_pages.clone(),
                lease: Some(lease),
            }));
        return true;
    }
    false
}

fn request_spooled_page(
    context: &VisiblePageLoadContext<'_>,
    placeholder: &gtk::Widget,
    index: u64,
    spool: bookie_app_services::stream_result_spool::StreamResultSpool,
) {
    let Some(lease) = context.resident_pages.reserve(bookie_core::MAX_QUERY_RESULT_BYTES) else {
        if let Some(container) = placeholder.parent().and_downcast::<gtk::Box>() {
            show_page_load_error(
                &container,
                placeholder,
                bookie_app_services::tr!(
                    "The result memory budget is full. Close or scroll away from other large results, then retry."
                )
                .to_string(),
            );
        } else {
            placeholder.remove_css_class("tp-stream-page-loading");
        }
        return;
    };
    if let Some(status) = placeholder.downcast_ref::<relm4::adw::StatusPage>() {
        status.set_description(Some(&bookie_app_services::tr!(
            "Loading this page from temporary storage…"
        )));
    }
    let _ = context
        .sender
        .send(super::SqlEditorInput::Stream(super::streaming::Input::LoadPage {
            generation: context.generation,
            index,
            identity: context.identity,
            spool,
            columns: context.columns.to_vec(),
            resident_pages: context.resident_pages.clone(),
            lease,
        }));
}

fn retire_previous_page(pages: &gtk::Stack, generation: u64, previous: Option<u64>, current: Option<u64>) {
    if let Some(previous) = previous
        && Some(previous) != current
        && let Some(previous_page) = pages.child_by_name(&page_widget_name(generation, previous))
        && let Some(current_page) = previous_page.first_child()
        && (!current_page.has_css_class("tp-stream-page-placeholder")
            || current_page.has_css_class("tp-stream-page-loading"))
    {
        let title = pages.page(&previous_page).title().unwrap_or_default().to_string();
        let placeholder = stream_page_placeholder_with_title(title);
        if let Ok(previous_page) = previous_page.downcast::<gtk::Box>() {
            replace_stream_page_content(&previous_page, &placeholder);
        }
    }
}

fn page_index_from_name(widget: &gtk::Widget, generation: u64) -> Option<u64> {
    let name = widget.widget_name();
    let (actual_generation, index) = name.strip_prefix(PAGE_PREFIX)?.split_once('-')?;
    if actual_generation.parse::<u64>().ok()? != generation {
        return None;
    }
    index.parse().ok()
}

pub(super) struct PageLoadContext<'a> {
    pub(super) holder: &'a gtk::Box,
    pub(super) generation: u64,
    pub(super) index: u64,
    pub(super) identity: super::StatementIdentity,
    pub(super) grid_sender: &'a relm4::Sender<GridMsg>,
    pub(super) connection_id: Option<uuid::Uuid>,
    pub(super) database: Arc<crate::services::database_service::DatabaseService>,
    pub(super) settings_store: LocalResultSettingsStore,
}

#[cfg(test)]
pub(super) fn complete_page_load(context: PageLoadContext<'_>, result: Result<Arc<QueryResult>, String>) {
    complete_page_load_retained(context, result, super::stream_pages::ResidentPageStore::default(), None);
}

pub(super) fn complete_page_load_retained(
    context: PageLoadContext<'_>,
    result: Result<Arc<QueryResult>, String>,
    resident_pages: super::stream_pages::ResidentPageStore,
    lease: Option<bookie_results::stream_page_budget::StreamPageLease>,
) {
    let root = context.holder.clone().upcast::<gtk::Widget>();
    let Some(container) = named::<gtk::Box>(&root, &page_widget_name(context.generation, context.index)) else {
        return;
    };
    let Some(current) = container.first_child() else {
        return;
    };
    if !current.has_css_class("tp-stream-page-loading") {
        return;
    }
    match result {
        Ok(page) => show_loaded_page(context, &root, &container, &current, page, resident_pages, lease),
        Err(message) => show_page_load_error(&container, &current, message),
    }
}

fn show_loaded_page(
    context: PageLoadContext<'_>,
    root: &gtk::Widget,
    container: &gtk::Box,
    current: &gtk::Widget,
    page: Arc<QueryResult>,
    resident_pages: super::stream_pages::ResidentPageStore,
    lease: Option<bookie_results::stream_page_budget::StreamPageLease>,
) {
    let Some(lease) = lease.or_else(|| resident_pages.retain(&page)) else {
        show_page_load_error(
            container,
            current,
            bookie_app_services::tr!("The result memory budget is full.").to_string(),
        );
        return;
    };
    let page = resident_pages.insert(context.index, page, lease, false);
    let widget = build_stream_page_widget(
        &page,
        context.grid_sender,
        context.connection_id,
        context.database,
        context.settings_store,
        super::outcomes::streamed_view_key(context.identity, context.generation, context.index as usize),
        named::<gtk::Widget>(root, "tp-result-find-entry").is_some(),
    );
    let title = container
        .parent()
        .and_downcast::<gtk::Stack>()
        .map(|pages| pages.page(container).title().unwrap_or_default().to_string())
        .unwrap_or_default();
    resident_pages.protect(context.index);
    let pages = resident_pages.clone();
    super::outcomes::stream_result_view::release_stream_page_on_unmap(container, &widget, title, move || {
        pages.unprotect(context.index)
    });
    replace_stream_page_content(container, &widget);
}

fn show_page_load_error(container: &gtk::Box, current: &gtk::Widget, message: String) {
    if let Some(status) = current.downcast_ref::<relm4::adw::StatusPage>() {
        status.set_title(&bookie_app_services::tr!("Could not load this result page"));
        status.set_description(Some(&crate::ui::plain_text::escape_markup(&message)));
        if let Some(pages) = container.parent().and_downcast::<gtk::Stack>() {
            let retry = gtk::Button::with_label(&bookie_app_services::tr!("Retry loading page"));
            let pages = pages.downgrade();
            retry.connect_clicked(move |_| {
                if let Some(pages) = pages.upgrade() {
                    pages.notify("visible-child");
                }
            });
            status.set_child(Some(&retry));
        }
    }
    current.remove_css_class("tp-stream-page-loading");
}

#[cfg(test)]
#[path = "stream_view_tests.rs"]
mod tests;
