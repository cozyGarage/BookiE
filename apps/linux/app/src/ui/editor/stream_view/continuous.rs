use std::sync::Arc;

use gtk4::prelude::*;
use relm4::gtk;

use super::super::stream_pages::StreamedResult;
use super::paged_model::PagedModel;
use crate::ui::grid::{GridMsg, TabGridContext, build_column_view};

pub(in crate::ui::editor) fn install(
    holder: &gtk::Box,
    header: &relm4::adw::WrapBox,
    pager: &gtk::Box,
    pages: &gtk::Stack,
    result: &StreamedResult,
    target: &super::LiveStreamTarget<'_>,
) {
    let Some(spool) = result.spool.clone() else { return };
    let counts = result.page_row_counts.clone();
    let columns = result.columns.clone();
    let button = gtk::ToggleButton::with_label(&bookie_app_services::tr!("Continuous scrolling"));
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.set_vexpand(true);
    container.set_visible(false);
    container.set_widget_name(&super::super::result_tools::continuous_container_name(
        result.generation,
    ));
    header.append(&button);
    holder.append(&container);
    let pager = pager.downgrade();
    let pages = pages.downgrade();
    let sender = target.grid_sender.clone();
    let connection_id = target.connection_id;
    let database = target.database.clone();
    let generation = std::rc::Rc::new(std::cell::Cell::new(0_u64));
    button.connect_toggled(move |button| {
        let active = button.is_active();
        if let Some(pager) = pager.upgrade() {
            pager.set_visible(!active);
        }
        if let Some(pages) = pages.upgrade() {
            pages.set_visible(!active);
        }
        container.set_visible(active);
        if !active {
            generation.set(generation.get().wrapping_add(1));
            while let Some(child) = container.first_child() {
                container.remove(&child);
            }
            return;
        }
        if container.first_child().is_some() {
            return;
        }
        let request = generation.get().wrapping_add(1);
        generation.set(request);
        let model = PagedModel::new(&counts, columns.len());
        start_view(
            &container,
            model,
            spool.clone(),
            columns.clone(),
            sender.clone(),
            (connection_id, database.clone()),
            (generation.clone(), request),
        );
    });
}

fn start_view(
    container: &gtk::Box,
    model: PagedModel,
    spool: bookie_app_services::stream_result_spool::StreamResultSpool,
    columns: Vec<bookie_core::ColumnInfo>,
    sender: relm4::Sender<GridMsg>,
    connection: (
        Option<uuid::Uuid>,
        Arc<crate::services::database_service::DatabaseService>,
    ),
    activation: (std::rc::Rc<std::cell::Cell<u64>>, u64),
) {
    let (generation, request) = activation;
    let (connection_id, database) = connection;
    let empty = Arc::new(bookie_core::QueryResult {
        columns: columns.clone(),
        rows: Vec::new().into(),
        truncated: false,
    });
    let container = container.downgrade();
    let runtime = relm4::spawn(async { tokio::runtime::Handle::current() });
    gtk::glib::MainContext::default().spawn_local(async move {
        let (Some(container), Ok(runtime)) = (container.upgrade(), runtime.await) else { return };
        if generation.get() != request {
            return;
        }
        let callback = model.visibility_callback(&runtime, &spool, &columns);
        let (view, selection) = build_column_view(
            &empty,
            &columns,
            "",
            None,
            crate::ui::grid::GridSenders {
                edit: None,
                menu: Some(sender),
                sort: None,
            },
            crate::ui::grid::GridEnv {
                connection_id,
                tab_ctx: TabGridContext {
                    result_model: Some(model.clone().upcast()),
                    row_visibility: Some(callback),
                    preview_result_values: true,
                    ..Default::default()
                },
                column_widths: None,
                highlight_settings: None,
                database,
            },
        );
        model.attach_selection(&selection, &view);
        let status = gtk::Label::new(None);
        status.set_wrap(true);
        model.attach_status(&status);
        let retry = gtk::Button::with_label(&bookie_app_services::tr!("Retry loading pages"));
        let weak_model = model.downgrade();
        retry.connect_clicked(move |_| {
            if let Some(model) = weak_model.upgrade() {
                model.retry_visible(&runtime, &spool, &columns);
            }
        });
        container.append(&status);
        container.append(&retry);
        container.append(&crate::ui::grid::wrap_grid_scrolled(&view));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::editor::stream_pages::{ResidentPageStore, StreamTerminal};

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn continuous_view_loads_and_displays_spooled_rows() {
        gtk::init().unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let spool = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default()
            .create()
            .unwrap();
        let column = bookie_core::ColumnInfo {
            name: "id".into(),
            data_type: "int8".into(),
            nullable: false,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        };
        for page_index in 0..2 {
            let page = Arc::new(bookie_core::QueryResult {
                columns: vec![column.clone()],
                rows: (0..100)
                    .map(|row| vec![bookie_core::Value::Int(page_index * 100 + row)])
                    .collect(),
                truncated: false,
            });
            runtime.block_on(spool.append_page(page_index as u64, page)).unwrap();
        }
        let result = StreamedResult {
            pages: vec![],
            page_indices: vec![],
            page_row_counts: vec![100, 100],
            page_row_starts: vec![1, 101],
            retained_rows: 0,
            columns: vec![column],
            total_pages: 2,
            total_rows: 200,
            terminal: StreamTerminal::Complete,
            export: None,
            generation: 0,
            spool: Some(spool),
            resident_pages: ResidentPageStore::default(),
        };
        let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let pager = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let pages = gtk::Stack::new();
        let (sender, _) = relm4::channel();
        let target = super::super::LiveStreamTarget {
            grid_sender: &sender,
            connection_id: None,
            database: Arc::new(crate::services::database_service::DatabaseService::new_isolated()),
            settings_store: Default::default(),
        };
        let header = relm4::adw::WrapBox::new();
        holder.append(&header);
        install(&holder, &header, &pager, &pages, &result, &target);
        let toggle = header.first_child().unwrap().downcast::<gtk::ToggleButton>().unwrap();
        let window = gtk::Window::builder()
            .default_width(600)
            .default_height(300)
            .child(&holder)
            .build();
        window.present();
        toggle.set_active(true);
        toggle.set_active(false);
        toggle.set_active(true);
        wait_for_text(holder.upcast_ref(), "0");
        assert_eq!(count_views(holder.upcast_ref()), 1);
        let view = find_view(holder.upcast_ref()).unwrap();
        let model = view
            .model()
            .unwrap()
            .downcast::<gtk::MultiSelection>()
            .unwrap()
            .model()
            .unwrap()
            .downcast::<PagedModel>()
            .unwrap();
        let model_weak = model.downgrade();
        drop(model);
        view.scroll_to(190, None, gtk::ListScrollFlags::FOCUS, None);
        wait_for_text(holder.upcast_ref(), "190");
        view.scroll_to(0, None, gtk::ListScrollFlags::FOCUS, None);
        wait_for_text(holder.upcast_ref(), "0");
        drop(view);
        assert!(!pager.is_visible());
        toggle.set_active(false);
        assert!(pager.is_visible());
        assert_eq!(count_views(holder.upcast_ref()), 0);
        assert!(model_weak.upgrade().is_none());
        toggle.set_active(true);
        wait_for_text(holder.upcast_ref(), "0");
        assert_eq!(count_views(holder.upcast_ref()), 1);
        toggle.set_active(false);
        assert_eq!(count_views(holder.upcast_ref()), 0);
        assert!(model_weak.upgrade().is_none());
        window.destroy();
    }

    fn wait_for_text(root: &gtk::Widget, text: &str) {
        let context = gtk::glib::MainContext::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            while context.iteration(false) {}
            if contains_text(root, text) {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "row {text} did not appear");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    fn find_view(root: &gtk::Widget) -> Option<gtk::ColumnView> {
        let mut pending = vec![root.clone()];
        while let Some(widget) = pending.pop() {
            if let Ok(view) = widget.clone().downcast::<gtk::ColumnView>() {
                return Some(view);
            }
            let mut child = widget.first_child();
            while let Some(next) = child {
                child = next.next_sibling();
                pending.push(next);
            }
        }
        None
    }

    fn count_views(root: &gtk::Widget) -> usize {
        let mut pending = vec![root.clone()];
        let mut count = 0;
        while let Some(widget) = pending.pop() {
            count += usize::from(widget.is::<gtk::ColumnView>());
            let mut child = widget.first_child();
            while let Some(next) = child {
                child = next.next_sibling();
                pending.push(next);
            }
        }
        count
    }

    fn contains_text(root: &gtk::Widget, expected: &str) -> bool {
        let mut pending = vec![root.clone()];
        while let Some(widget) = pending.pop() {
            if widget
                .downcast_ref::<gtk::Label>()
                .is_some_and(|label| label.text() == expected)
            {
                return true;
            }
            let mut child = widget.first_child();
            while let Some(next) = child {
                child = next.next_sibling();
                pending.push(next);
            }
        }
        false
    }
}
