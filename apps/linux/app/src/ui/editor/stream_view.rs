mod pager;
pub(super) use pager::build as build_pager;

use relm4::gtk;
use relm4::gtk::prelude::*;
use std::sync::Arc;
use tablepro_core::QueryResult;

use super::StatementOutcome;
use super::outcomes::stream_result_view::{
    build_stream_page_widget, replace_stream_page_content, result_view_key, stream_page_container, stream_page_label,
    stream_page_placeholder, stream_page_placeholder_with_title, stream_progress_text,
};
use super::stream_pages::StreamedResult;
use crate::ui::grid::GridMsg;
use crate::ui::grid::local_filter::LocalResultSettingsStore;

pub(super) const LIVE_PAGES: &str = "tp-live-stream-pages";
pub(super) const LIVE_PROGRESS: &str = "tp-live-stream-progress";
const PAGE_PREFIX: &str = "tp-stream-page-";

pub(super) fn page_widget_name(generation: u64, page_index: u64) -> String {
    format!("{PAGE_PREFIX}{generation}-{page_index}")
}

fn named<T: IsA<gtk::Widget>>(root: &gtk::Widget, name: &str) -> Option<T> {
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        if widget.widget_name() == name
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
) -> bool {
    let root = holder.clone().upcast::<gtk::Widget>();
    let (Some(pages), Some(progress)) = (
        named::<gtk::Stack>(&root, LIVE_PAGES),
        named::<gtk::Label>(&root, LIVE_PROGRESS),
    ) else {
        return false;
    };
    let first_resident = result.page_indices.first().copied().unwrap_or(result.total_pages);
    let mut child = pages.first_child();
    while let Some(page) = child {
        child = page.next_sibling();
        let Some(index) = page
            .widget_name()
            .strip_prefix(PAGE_PREFIX)
            .and_then(|name| name.rsplit_once('-'))
            .and_then(|(_, index)| index.parse::<u64>().ok())
        else {
            continue;
        };
        if index >= first_resident {
            break;
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
    sender: &relm4::ComponentSender<super::SqlEditor>,
) {
    let Some(spool) = result.spool.clone() else {
        return;
    };
    let columns = result.columns.clone();
    let generation = result.generation;
    let sender = sender.clone();
    let previous_index = std::rc::Rc::new(std::cell::Cell::new(
        pages
            .visible_child()
            .and_then(|child| page_index_from_name(&child, generation)),
    ));
    pages.connect_visible_child_notify(move |pages| {
        let Some(container) = pages.visible_child() else {
            return;
        };
        let current_index = page_index_from_name(&container, generation);
        if let Some(previous) = previous_index.replace(current_index)
            && Some(previous) != current_index
            && let Some(previous_page) = pages.child_by_name(&page_widget_name(generation, previous))
            && !previous_page.has_css_class("tp-stream-page-ram-resident")
        {
            let title = pages.page(&previous_page).title().unwrap_or_default().to_string();
            let placeholder = stream_page_placeholder_with_title(title);
            if let Ok(previous_page) = previous_page.downcast::<gtk::Box>() {
                replace_stream_page_content(&previous_page, &placeholder);
            }
        }
        let Some(placeholder) = container.first_child() else {
            return;
        };
        if !placeholder.has_css_class("tp-stream-page-placeholder")
            || placeholder.has_css_class("tp-stream-page-loading")
        {
            return;
        }
        let Some(index) = container
            .widget_name()
            .strip_prefix(PAGE_PREFIX)
            .and_then(|name| name.rsplit_once('-'))
            .and_then(|(_, index)| index.parse::<u64>().ok())
        else {
            return;
        };
        placeholder.add_css_class("tp-stream-page-loading");
        if let Some(status) = placeholder.downcast_ref::<relm4::adw::StatusPage>() {
            status.set_description(Some(&crate::tr!("Loading this page from temporary storage…")));
        }
        sender.input(super::SqlEditorInput::Stream(super::streaming::Input::LoadPage {
            generation,
            index,
            identity,
            spool: spool.clone(),
            columns: columns.clone(),
        }));
    });
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

pub(super) fn complete_page_load(context: PageLoadContext<'_>, result: Result<Arc<QueryResult>, String>) {
    let PageLoadContext {
        holder,
        generation,
        index,
        identity,
        grid_sender,
        connection_id,
        database,
        settings_store,
    } = context;
    let root = holder.clone().upcast::<gtk::Widget>();
    let Some(container) = named::<gtk::Box>(&root, &page_widget_name(generation, index)) else {
        return;
    };
    let Some(current) = container.first_child() else {
        return;
    };
    if !current.has_css_class("tp-stream-page-loading") {
        return;
    }
    match result {
        Ok(page) => {
            let widget = build_stream_page_widget(
                &page,
                grid_sender,
                connection_id,
                database,
                settings_store,
                result_view_key(identity, index as usize),
            );
            replace_stream_page_content(&container, &widget);
        }
        Err(message) => {
            if let Some(status) = current.downcast_ref::<relm4::adw::StatusPage>() {
                status.set_title(&crate::tr!("Could not load this result page"));
                status.set_description(Some(&message));
            }
            current.remove_css_class("tp-stream-page-loading");
        }
    }
}

#[cfg(test)]
#[path = "stream_view_tests.rs"]
mod tests;
