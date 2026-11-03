use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{gio, glib};
use std::cell::{Cell, RefCell};

use super::page_index::PageIndex;
use super::page_window::PageWindow;
use crate::ui::row_object::RowObject;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct PagedModel {
        pub(super) index: RefCell<PageIndex>,
        pub(super) width: Cell<usize>,
        pub(super) pages: RefCell<Option<PageWindow<std::sync::Arc<bookie_core::QueryResult>>>>,
        pub(super) selection: glib::WeakRef<gtk4::MultiSelection>,
        pub(super) view: glib::WeakRef<gtk4::ColumnView>,
        pub(super) status: glib::WeakRef<gtk4::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PagedModel {
        const NAME: &str = "BookiePagedResultModel";
        type Type = super::PagedModel;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for PagedModel {}

    impl ListModelImpl for PagedModel {
        fn item_type(&self) -> glib::Type {
            RowObject::static_type()
        }

        fn n_items(&self) -> u32 {
            u32::try_from(self.index.borrow().len()).unwrap_or(u32::MAX)
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            let (page, offset) = self.index.borrow().row(position as usize)?;
            let pages = self.pages.borrow();
            let row = pages
                .as_ref()
                .and_then(|pages| pages.get(page))
                .and_then(|result| RowObject::from_shared(result, offset, None));
            Some(row.unwrap_or_else(|| RowObject::unloaded(self.width.get())).upcast())
        }
    }
}

glib::wrapper! {
    pub struct PagedModel(ObjectSubclass<imp::PagedModel>) @implements gio::ListModel;
}

impl PagedModel {
    pub(super) fn new(counts: &[usize], width: usize) -> Self {
        Self::with_limit(counts, width, bookie_core::MAX_QUERY_RESULT_BYTES)
    }

    fn with_limit(counts: &[usize], width: usize, limit: usize) -> Self {
        let model: Self = glib::Object::new();
        *model.imp().index.borrow_mut() = PageIndex::new(counts);
        model.imp().width.set(width);
        *model.imp().pages.borrow_mut() = Some(PageWindow::new(limit, 2));
        model
    }

    fn show(&self, start: usize, count: usize) {
        let wanted = self.imp().index.borrow().pages_in(start, count);
        if let Some(window) = self.imp().pages.borrow_mut().as_mut() {
            window.show(&wanted);
        }
        self.sync_status();
    }

    fn next_request(&self) -> Option<u64> {
        self.imp().pages.borrow_mut().as_mut()?.next_request()
    }

    pub(super) fn visibility_callback(
        &self,
        runtime: &tokio::runtime::Handle,
        spool: &bookie_app_services::stream_result_spool::StreamResultSpool,
        columns: &[bookie_core::ColumnInfo],
    ) -> std::rc::Rc<dyn Fn(u32, bool)> {
        let rows = std::rc::Rc::new(RefCell::new(std::collections::BTreeSet::new()));
        let scheduled = std::rc::Rc::new(Cell::new(false));
        let model = self.downgrade();
        let runtime = runtime.clone();
        let spool = spool.clone();
        let columns = columns.to_vec();
        std::rc::Rc::new(move |position, visible| {
            if visible {
                rows.borrow_mut().insert(position);
            } else {
                rows.borrow_mut().remove(&position);
            }
            if scheduled.replace(true) {
                return;
            }
            let rows = rows.clone();
            let scheduled = scheduled.clone();
            let model = model.clone();
            let runtime = runtime.clone();
            let spool = spool.clone();
            let columns = columns.clone();
            glib::idle_add_local_once(move || {
                scheduled.set(false);
                let Some(model) = model.upgrade() else { return };
                let rows = rows.borrow();
                let Some((&first, &last)) = rows.first().zip(rows.last()) else {
                    return;
                };
                model.show(first as usize, (last - first) as usize + 1);
                model.load_from_spool(&runtime, &spool, &columns);
            });
        })
    }

    fn load_from_spool(
        &self,
        runtime: &tokio::runtime::Handle,
        spool: &bookie_app_services::stream_result_spool::StreamResultSpool,
        columns: &[bookie_core::ColumnInfo],
    ) {
        while let Some(index) = self.next_request() {
            let (send, receive) = async_channel::bounded(1);
            let worker_spool = spool.clone();
            let worker_columns = columns.to_vec();
            runtime.spawn(async move {
                let _ = send.send(worker_spool.load_page(index, worker_columns).await).await;
            });
            let model = self.downgrade();
            let runtime = runtime.clone();
            let spool = spool.clone();
            let columns = columns.to_vec();
            glib::MainContext::ref_thread_default().spawn_local(async move {
                let result = receive.recv().await;
                let Some(model) = model.upgrade() else { return };
                match result {
                    Ok(Ok(page)) => {
                        model.install(index, page);
                    }
                    _ => {
                        if let Some(window) = model.imp().pages.borrow_mut().as_mut() {
                            window.failed(index);
                        }
                        model.show_failure();
                    }
                }
                model.load_from_spool(&runtime, &spool, &columns);
            });
        }
    }

    fn install(&self, index: u64, page: std::sync::Arc<bookie_core::QueryResult>) -> bool {
        let Some((start, count)) = self.imp().index.borrow().page_range(index) else {
            return false;
        };
        if count != page.rows.len() || page.rows.iter().any(|row| row.len() != self.imp().width.get()) {
            if let Some(window) = self.imp().pages.borrow_mut().as_mut() {
                window.failed(index);
            }
            self.show_failure();
            return false;
        }
        let size = page.estimated_row_bytes();
        let evicted = match self.imp().pages.borrow_mut().as_mut() {
            Some(pages) => match pages.loaded(index, page, size) {
                Some(evicted) => evicted,
                None => return false,
            },
            None => return false,
        };
        for page in evicted {
            if let Some((start, count)) = self.imp().index.borrow().page_range(page) {
                self.refresh(start, count);
            }
        }
        self.refresh(start, count);
        self.sync_status();
        true
    }

    fn refresh(&self, start: usize, count: usize) {
        let selection = self.imp().selection.upgrade();
        let selected = selection.as_ref().map(|selection| selection.selection().copy());
        let focus = self
            .imp()
            .view
            .upgrade()
            .and_then(|view| crate::ui::grid::focused_row_position(&view));
        self.items_changed(start as u32, count as u32, count as u32);
        if let (Some(selection), Some(selected)) = (selection, selected) {
            selection.set_selection(&selected, &gtk4::Bitset::new_range(start as u32, count as u32));
            if let Some(position) =
                focus.filter(|position| (*position as usize) >= start && (*position as usize) < start + count)
            {
                let view = self.imp().view.clone();
                glib::idle_add_local_once(move || {
                    let Some(view) = view.upgrade() else { return };
                    let Some(window) = view.root().and_then(|root| root.downcast::<gtk4::Window>().ok()) else {
                        return;
                    };
                    let focused = gtk4::prelude::GtkWindowExt::focus(&window);
                    if view.is_mapped()
                        && focused.as_ref().map_or(window.is_active(), |focused| {
                            focused == &view || focused.is_ancestor(&view) || view.is_ancestor(focused)
                        })
                    {
                        view.scroll_to(position, None, gtk4::ListScrollFlags::FOCUS, None);
                    }
                });
            }
        }
    }

    pub(super) fn attach_selection(&self, selection: &gtk4::MultiSelection, view: &gtk4::ColumnView) {
        self.imp().selection.set(Some(selection));
        self.imp().view.set(Some(view));
    }

    pub(super) fn attach_status(&self, status: &gtk4::Label) {
        self.imp().status.set(Some(status));
    }

    fn show_failure(&self) {
        self.sync_status();
    }

    fn sync_status(&self) {
        let Some(status) = self.imp().status.upgrade() else {
            return;
        };
        let pages = self.imp().pages.borrow();
        let Some(pages) = pages.as_ref() else { return };
        let message = if pages.has_failed() {
            crate::tr!("A saved result page could not be loaded. Retry or use the page view.")
        } else if pages.has_deferred() {
            crate::tr!("Some visible pages exceed the memory limit. Use the page view to see all rows.")
        } else {
            String::new()
        };
        status.set_text(&message);
    }

    pub(super) fn retry_visible(
        &self,
        runtime: &tokio::runtime::Handle,
        spool: &bookie_app_services::stream_result_spool::StreamResultSpool,
        columns: &[bookie_core::ColumnInfo],
    ) {
        if let Some(window) = self.imp().pages.borrow_mut().as_mut() {
            window.retry_visible();
        }
        self.sync_status();
        self.load_from_spool(runtime, spool, columns);
    }
}

#[cfg(test)]
#[path = "paged_model_tests.rs"]
mod tests;
