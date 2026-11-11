use super::*;
pub(in crate::ui::editor) use super::{build_stream_page_widget, streamed_view_key};

#[cfg(test)]
thread_local! { pub(super) static PAGE_ROW_INDEX_LOOKUPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

pub(in crate::ui::editor) fn build_streamed_result_widget(
    outcome: &StatementOutcome,
    result: &StreamedResult,
    grid_sender: &relm4::Sender<GridMsg>,
    connection_id: Option<uuid::Uuid>,
    database: std::sync::Arc<crate::services::database_service::DatabaseService>,
    settings_store: LocalResultSettingsStore,
    sender: Option<&relm4::ComponentSender<super::super::SqlEditor>>,
) -> gtk::Widget {
    let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let target = super::super::stream_view::LiveStreamTarget {
        grid_sender,
        connection_id,
        database: database.clone(),
        settings_store: settings_store.clone(),
    };
    let page_switcher = build_page_stack(outcome, result, &target, sender);
    let progress = gtk::Label::new(Some(&stream_progress_text(result)));
    if matches!(result.terminal, StreamTerminal::Loading) {
        page_switcher.set_widget_name(super::super::stream_view::LIVE_PAGES);
        progress.set_widget_name(super::super::stream_view::LIVE_PROGRESS);
    } else {
        page_switcher.set_widget_name(&super::super::stream_view::completed_page_stack_name(result.generation));
    }
    let header = super::super::stream_view::build_header(&progress);
    holder.append(&header);
    if let Some(notice) = stream_terminal_notice(result) {
        let banner = adw::Banner::new(&notice);
        banner.set_use_markup(false);
        banner.set_revealed(true);
        holder.append(&banner);
    }
    let pager = super::super::stream_view::build_pager(&page_switcher);
    header.append(&pager);
    append_result_tools(&holder, &header, &page_switcher, outcome, result, sender);
    append_stream_export(&header, result, grid_sender);
    if matches!(result.terminal, StreamTerminal::Complete) && sender.is_some() {
        super::super::stream_view::install_continuous(&holder, &header, &pager, &page_switcher, result, &target);
    }
    holder.append(&page_switcher);
    holder.upcast()
}

fn build_page_stack(
    outcome: &StatementOutcome,
    result: &StreamedResult,
    target: &super::super::stream_view::LiveStreamTarget<'_>,
    sender: Option<&relm4::ComponentSender<super::super::SqlEditor>>,
) -> gtk::Stack {
    let page_switcher = gtk::Stack::builder().vexpand(true).hexpand(true).build();
    for page_index in 0..result.total_pages {
        let container = stream_page_container(result, outcome.identity, page_index, target);
        page_switcher.add_titled(
            &container,
            Some(container.widget_name().as_str()),
            &stream_page_label(result, page_index as usize),
        );
    }
    if stream_terminal_notice(result).is_none()
        && let Some(page) = stream_terminal_page(result, outcome.identity.statement_index)
    {
        page_switcher.add_titled(&page, Some("status"), &bookie_app_services::tr!("Status"));
    }
    let initial = if matches!(result.terminal, StreamTerminal::Loading | StreamTerminal::Complete) {
        page_switcher.first_child()
    } else {
        page_switcher.last_child()
    };
    if let Some(initial) = initial {
        page_switcher.set_visible_child(&initial);
    }
    if let Some(sender) = sender {
        super::super::stream_view::install_page_loader(&page_switcher, result, outcome.identity, sender.input_sender());
    }
    page_switcher
}

fn append_result_tools(
    holder: &gtk::Box,
    header: &adw::WrapBox,
    pages: &gtk::Stack,
    outcome: &StatementOutcome,
    result: &StreamedResult,
    sender: Option<&relm4::ComponentSender<super::super::SqlEditor>>,
) {
    let (StreamTerminal::Complete, Some(sender)) = (&result.terminal, sender) else {
        return;
    };
    super::super::result_tools::append(super::super::result_tools::ToolsTarget {
        holder,
        header,
        pages,
        result,
        identity: super::super::result_tools::stream_identity(outcome.identity, result.generation),
        sender: sender.input_sender(),
    });
}

fn append_stream_export(holder: &adw::WrapBox, result: &StreamedResult, grid_sender: &relm4::Sender<GridMsg>) {
    if matches!(result.terminal, StreamTerminal::Loading) {
        let notice = gtk::Label::new(Some(&bookie_app_services::tr!(
            "Export all rows is available when the query completes."
        )));
        notice.set_ellipsize(gtk::pango::EllipsizeMode::End);
        notice.set_max_width_chars(40);
        holder.append(&notice);
    }
    if matches!(result.terminal, StreamTerminal::Complete)
        && let Some(request) = result.export.clone().filter(|request| request.spool.is_some())
    {
        let export = gtk::Button::with_label(&bookie_app_services::tr!("Export all rows…"));
        export.add_css_class("flat");
        export.connect_clicked({
            let grid_sender = grid_sender.clone();
            move |_| {
                let _ = grid_sender.send(GridMsg::ExportStreamedResults(request.clone()));
            }
        });
        holder.append(&export);
    }
}

pub(in crate::ui::editor) fn stream_page_container(
    result: &StreamedResult,
    identity: StatementIdentity,
    page_index: u64,
    target: &super::super::stream_view::LiveStreamTarget<'_>,
) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.set_widget_name(&super::super::stream_view::page_widget_name(
        result.generation,
        page_index,
    ));
    let resident = result
        .page_indices
        .binary_search(&page_index)
        .ok()
        .and_then(|position| result.pages.get(position))
        .and_then(std::sync::Weak::upgrade);
    if let Some(page) = resident {
        result.resident_pages.protect(page_index);
        container.add_css_class("tp-stream-page-ram-resident");
        let grid = build_stream_page_widget(
            &page,
            target.grid_sender,
            target.connection_id,
            target.database.clone(),
            target.settings_store.clone(),
            streamed_view_key(identity, result.generation, page_index as usize),
            offers_whole_result_filter(result),
        );
        let title = stream_page_label(result, page_index as usize);
        let resident_pages = result.resident_pages.clone();
        release_stream_page_on_unmap(&container, &grid, title, move || {
            resident_pages.unprotect(page_index);
        });
        container.append(&grid);
    } else {
        container.append(&stream_page_placeholder(result, page_index));
    }
    container
}

pub(in crate::ui::editor) fn release_stream_page_on_unmap(
    container: &gtk::Box,
    grid: &gtk::Widget,
    title: String,
    on_release: impl Fn() + 'static,
) {
    let released = std::rc::Rc::new(std::cell::Cell::new(false));
    let release = std::rc::Rc::new(move || {
        if !released.replace(true) {
            on_release();
        }
    });
    let weak_grid = grid.downgrade();
    let weak_container = container.downgrade();
    let unmap_release = release.clone();
    grid.connect_unmap(move |_| {
        let weak_grid = weak_grid.clone();
        let weak_container = weak_container.clone();
        let title = title.clone();
        let release = unmap_release.clone();
        gtk::glib::idle_add_local_once(move || {
            let Some(grid) = weak_grid.upgrade() else {
                release();
                return;
            };
            if grid.is_mapped() {
                return;
            }
            let Some(container) = weak_container.upgrade() else {
                release();
                return;
            };
            if container.first_child().as_ref() == Some(&grid) {
                replace_stream_page_content(&container, &stream_page_placeholder_with_title(title));
            }
            release();
            if let Some(pages) = container.parent().and_downcast::<gtk::Stack>()
                && pages.is_mapped()
                && pages
                    .visible_child()
                    .as_ref()
                    .is_some_and(|child| child == container.upcast_ref::<gtk::Widget>())
            {
                pages.notify("visible-child");
            }
        });
    });
    grid.connect_destroy(move |_| release());
}

pub(in crate::ui::editor) fn replace_stream_page_content(container: &gtk::Box, page: &gtk::Widget) {
    clear_box(container);
    container.append(page);
}

pub(in crate::ui::editor) fn stream_page_placeholder(result: &StreamedResult, page_index: u64) -> gtk::Widget {
    stream_page_placeholder_with_title(stream_page_label(result, page_index as usize))
}

pub(in crate::ui::editor) fn stream_page_placeholder_with_title(title: String) -> gtk::Widget {
    let page = adw::StatusPage::builder()
        .title(title)
        .description(bookie_app_services::tr!(
            "Select this page to load it from temporary storage."
        ))
        .icon_name("document-open-recent-symbolic")
        .vexpand(true)
        .build();
    page.add_css_class("tp-stream-page-placeholder");
    page.upcast()
}

pub(in crate::ui::editor) fn stream_progress_text(result: &StreamedResult) -> String {
    let rows = bookie_app_services::trn!("Received {rows} row", "Received {rows} rows", result.total_rows).replace(
        "{rows}",
        &crate::ui::count_format::format_thousands(result.total_rows as u128),
    );
    let pages = bookie_app_services::trn!("{pages} page", "{pages} pages", result.total_pages).replace(
        "{pages}",
        &crate::ui::count_format::format_thousands(u128::from(result.total_pages)),
    );
    let status = match result.terminal {
        StreamTerminal::Loading => bookie_app_services::tr!("Receiving…"),
        StreamTerminal::Complete => bookie_app_services::tr!("Complete"),
        StreamTerminal::Cancelled => bookie_app_services::tr!("Cancelled"),
        StreamTerminal::TimedOut => bookie_app_services::tr!("Timed out"),
        StreamTerminal::Failed(_) => bookie_app_services::tr!("Incomplete"),
    };
    let received = format!("{status} · {rows} · {pages}");
    if stream_retained_rows(result) == result.total_rows {
        return received;
    }
    let notice = bookie_app_services::tr!("Earlier pages can be loaded from temporary storage.");
    format!("{received}\n{notice}")
}

fn stream_retained_rows(result: &StreamedResult) -> usize {
    #[cfg(test)]
    RETAINED_ROW_COUNT_LOOKUPS.with(|visits| visits.set(visits.get() + 1));
    result.retained_rows
}

#[cfg(test)]
thread_local! {
    pub(super) static RETAINED_ROW_COUNT_LOOKUPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(in crate::ui::editor) fn stream_page_label(result: &StreamedResult, page_index: usize) -> String {
    let Some(count) = stream_page_row_count(result, page_index).filter(|count| *count > 0) else {
        return bookie_app_services::tr!("Rows 0");
    };
    let Some(start) = stream_page_row_start(result, page_index) else {
        return bookie_app_services::tr!("Rows 0");
    };
    let end = start.saturating_add(count.saturating_sub(1));
    bookie_app_services::tr!("Rows {start}-{end}")
        .replace("{start}", &crate::ui::count_format::format_thousands(start as u128))
        .replace("{end}", &crate::ui::count_format::format_thousands(end as u128))
}

fn stream_page_row_count(result: &StreamedResult, page_index: usize) -> Option<usize> {
    #[cfg(test)]
    PAGE_ROW_INDEX_LOOKUPS.with(|visits| visits.set(visits.get() + 1));
    result.page_row_counts.get(page_index).copied()
}

fn stream_page_row_start(result: &StreamedResult, page_index: usize) -> Option<usize> {
    #[cfg(test)]
    PAGE_ROW_INDEX_LOOKUPS.with(|visits| visits.set(visits.get() + 1));
    result.page_row_starts.get(page_index).copied()
}

fn stream_terminal_notice(result: &StreamedResult) -> Option<String> {
    if result.total_pages == 0 {
        return None;
    }
    match result.terminal {
        StreamTerminal::TimedOut => Some(bookie_app_services::tr!(
            "Query timed out. All received pages remain available."
        )),
        StreamTerminal::Cancelled => Some(bookie_app_services::tr!(
            "Query cancelled. All received pages remain available."
        )),
        _ => None,
    }
}

fn stream_terminal_page(result: &StreamedResult, statement_index: usize) -> Option<gtk::Widget> {
    match &result.terminal {
        StreamTerminal::Loading | StreamTerminal::Complete => None,
        StreamTerminal::Failed(message) => Some(error_page(statement_index, message).upcast()),
        StreamTerminal::Cancelled => Some(
            adw::StatusPage::builder()
                .title(bookie_app_services::tr!("Query cancelled"))
                .description(bookie_app_services::tr!("All received pages remain available."))
                .icon_name("process-stop-symbolic")
                .vexpand(true)
                .build()
                .upcast(),
        ),
        StreamTerminal::TimedOut => Some(
            adw::StatusPage::builder()
                .title(bookie_app_services::tr!("Query timed out"))
                .description(bookie_app_services::tr!("All received pages remain available."))
                .icon_name("dialog-warning-symbolic")
                .vexpand(true)
                .build()
                .upcast(),
        ),
    }
}

pub(in crate::ui::editor) fn offers_whole_result_filter(result: &StreamedResult) -> bool {
    matches!(result.terminal, StreamTerminal::Complete) && result.spool.is_some()
}
