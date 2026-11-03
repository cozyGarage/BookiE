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
        true
    }

    fn refresh(&self, start: usize, count: usize) {
        let selection = self.imp().selection.upgrade();
        let selected = selection.as_ref().map(|selection| selection.selection().copy());
        self.items_changed(start as u32, count as u32, count as u32);
        if let (Some(selection), Some(selected)) = (selection, selected) {
            selection.set_selection(&selected, &gtk4::Bitset::new_range(start as u32, count as u32));
        }
    }

    pub(super) fn attach_selection(&self, selection: &gtk4::MultiSelection) {
        self.imp().selection.set(Some(selection));
    }

    pub(super) fn attach_status(&self, status: &gtk4::Label) {
        self.imp().status.set(Some(status));
    }

    fn show_failure(&self) {
        if let Some(status) = self.imp().status.upgrade() {
            status.set_text(&crate::tr!(
                "A saved result page could not be loaded. Retry or use the page view."
            ));
        }
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
        if let Some(status) = self.imp().status.upgrade() {
            status.set_text("");
        }
        self.load_from_spool(runtime, spool, columns);
    }
}

#[test]
fn result_model_maps_global_positions_without_allocating_rows() {
    let model = PagedModel::new(&[2, 0, 1], 1);
    assert_eq!(model.n_items(), 3);
    let row = model
        .item(2)
        .unwrap()
        .downcast::<crate::ui::row_object::RowObject>()
        .unwrap();
    assert!(row.complete_cells().is_none());
    assert!(model.item(3).is_none());
}

#[test]
fn result_model_reads_loaded_pages_and_refuses_wrong_page_lengths() {
    let model = PagedModel::new(&[2, 1], 1);
    let page = std::sync::Arc::new(bookie_core::QueryResult {
        columns: Vec::new(),
        rows: vec![vec![bookie_core::Value::Int(10)], vec![bookie_core::Value::Null]].into(),
        truncated: false,
    });
    assert!(!model.install(1, page.clone()));
    model.show(0, 2);
    assert_eq!(model.next_request(), Some(0));
    assert!(model.install(0, page));
    let row = model.item(0).unwrap().downcast::<RowObject>().unwrap();
    assert_eq!(row.complete_cells(), Some(vec![bookie_core::Value::Int(10)]));
    let null = model.item(1).unwrap().downcast::<RowObject>().unwrap();
    assert_eq!(null.complete_cells(), Some(vec![bookie_core::Value::Null]));
    assert!(
        model
            .item(2)
            .unwrap()
            .downcast::<RowObject>()
            .unwrap()
            .complete_cells()
            .is_none()
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn loading_a_page_preserves_the_selected_row() {
    gtk4::init().unwrap();
    let model = PagedModel::new(&[2], 1);
    let selection = gtk4::MultiSelection::new(Some(model.clone()));
    model.imp().selection.set(Some(&selection));
    model.show(0, 2);
    assert_eq!(model.next_request(), Some(0));
    assert!(selection.select_item(1, false));
    let page = std::sync::Arc::new(bookie_core::QueryResult {
        columns: Vec::new(),
        rows: vec![vec![bookie_core::Value::Int(10)], vec![bookie_core::Value::Int(20)]].into(),
        truncated: false,
    });
    assert!(model.install(0, page));
    assert!(selection.is_selected(1));
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn evicting_a_page_preserves_selection() {
    gtk4::init().unwrap();
    let model = PagedModel::with_limit(&[1, 1], 1, 1);
    let selection = gtk4::MultiSelection::new(Some(model.clone()));
    model.imp().selection.set(Some(&selection));
    let page = |value| {
        std::sync::Arc::new(bookie_core::QueryResult {
            columns: Vec::new(),
            rows: vec![vec![bookie_core::Value::Int(value)]].into(),
            truncated: false,
        })
    };
    model.show(0, 1);
    assert_eq!(model.next_request(), Some(0));
    assert!(model.install(0, page(1)));
    assert!(selection.select_item(0, false));
    model.show(1, 1);
    assert_eq!(model.next_request(), Some(1));
    assert!(model.install(1, page(2)));
    assert!(selection.is_selected(0));
}

#[test]
fn moving_the_window_evicts_old_values_and_allows_reloading() {
    let model = PagedModel::with_limit(&[1, 1], 1, 1);
    let first = std::sync::Arc::new(bookie_core::QueryResult {
        columns: Vec::new(),
        rows: vec![vec![bookie_core::Value::Text("first".into())]].into(),
        truncated: false,
    });
    let second = std::sync::Arc::new(bookie_core::QueryResult {
        columns: Vec::new(),
        rows: vec![vec![bookie_core::Value::Text("second".into())]].into(),
        truncated: false,
    });
    model.show(0, 1);
    assert_eq!(model.next_request(), Some(0));
    assert!(model.install(0, first.clone()));
    model.show(1, 1);
    assert_eq!(model.next_request(), Some(1));
    assert!(model.install(1, second));
    let old = model.item(0).unwrap().downcast::<RowObject>().unwrap();
    assert!(old.complete_cells().is_none());
    model.show(0, 1);
    assert_eq!(model.next_request(), Some(0));
    assert!(model.install(0, first));
    let reloaded = model.item(0).unwrap().downcast::<RowObject>().unwrap();
    assert_eq!(
        reloaded.complete_cells(),
        Some(vec![bookie_core::Value::Text("first".into())])
    );
}

#[test]
fn spool_page_loads_without_blocking_the_gtk_context() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let spool = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default()
        .create()
        .unwrap();
    let page = std::sync::Arc::new(bookie_core::QueryResult {
        columns: Vec::new(),
        rows: vec![vec![bookie_core::Value::Int(42)]].into(),
        truncated: false,
    });
    runtime.block_on(spool.append_page(0, page)).unwrap();
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| {
            let model = PagedModel::new(&[1], 1);
            model.show(0, 1);
            model.load_from_spool(&runtime.handle().clone(), &spool, &[]);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                while context.iteration(false) {}
                let row = model.item(0).unwrap().downcast::<RowObject>().unwrap();
                if row.complete_cells() == Some(vec![bookie_core::Value::Int(42)]) {
                    break;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        })
        .unwrap();
}

#[test]
fn spool_loader_starts_waiting_pages_after_inflight_reads_finish() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let spool = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default()
        .create()
        .unwrap();
    for index in 0..3 {
        let page = std::sync::Arc::new(bookie_core::QueryResult {
            columns: Vec::new(),
            rows: vec![vec![bookie_core::Value::Int(index)]].into(),
            truncated: false,
        });
        runtime.block_on(spool.append_page(index as u64, page)).unwrap();
    }
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| {
            let model = PagedModel::new(&[1, 1, 1], 1);
            model.show(0, 3);
            model.load_from_spool(runtime.handle(), &spool, &[]);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                while context.iteration(false) {}
                let row = model.item(2).unwrap().downcast::<RowObject>().unwrap();
                if row.complete_cells() == Some(vec![bookie_core::Value::Int(2)]) {
                    break;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        })
        .unwrap();
}

#[test]
fn spool_read_failure_can_be_retried_without_reloading_in_a_loop() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let spool = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default()
        .create()
        .unwrap();
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| {
            let model = PagedModel::new(&[1], 1);
            model.show(0, 1);
            model.load_from_spool(runtime.handle(), &spool, &[]);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                while context.iteration(false) {}
                if model.imp().pages.borrow().as_ref().unwrap().is_failed(0) {
                    break;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert!(model.next_request().is_none());
            model.imp().pages.borrow_mut().as_mut().unwrap().retry(0);
            assert_eq!(model.next_request(), Some(0));
        })
        .unwrap();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn failed_spool_read_shows_status_and_retry_loads_the_page() {
    gtk4::init().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let spool = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default()
        .create()
        .unwrap();
    let model = PagedModel::new(&[1], 1);
    let status = gtk4::Label::new(None);
    model.attach_status(&status);
    model.show(0, 1);
    model.load_from_spool(runtime.handle(), &spool, &[]);
    wait_for_condition(|| !status.text().is_empty());
    let page = std::sync::Arc::new(bookie_core::QueryResult {
        columns: Vec::new(),
        rows: vec![vec![bookie_core::Value::Int(42)]].into(),
        truncated: false,
    });
    runtime.block_on(spool.append_page(0, page)).unwrap();
    model.retry_visible(runtime.handle(), &spool, &[]);
    wait_for_condition(|| {
        model
            .item(0)
            .and_downcast::<RowObject>()
            .and_then(|row| row.complete_cells())
            == Some(vec![bookie_core::Value::Int(42)])
    });
    assert!(status.text().is_empty());
}

#[cfg(test)]
fn wait_for_condition(condition: impl Fn() -> bool) {
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !condition() {
        while context.iteration(false) {}
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn closing_a_model_does_not_wait_for_an_inflight_spool_read() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let spool = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default()
        .create()
        .unwrap();
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| {
            let model = PagedModel::new(&[1], 1);
            model.show(0, 1);
            model.load_from_spool(runtime.handle(), &spool, &[]);
            let weak = model.downgrade();
            drop(model);
            assert!(weak.upgrade().is_none());
            runtime.block_on(async { tokio::task::yield_now().await });
            while context.iteration(false) {}
            assert!(weak.upgrade().is_none());
        })
        .unwrap();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn result_grid_uses_the_paged_model_for_its_rows() {
    gtk4::init().unwrap();
    let model = PagedModel::new(&[2], 1);
    let result = std::sync::Arc::new(bookie_core::QueryResult {
        columns: Vec::new(),
        rows: Vec::new().into(),
        truncated: false,
    });
    let (view, selection) = crate::ui::grid::build_column_view(
        &result,
        &[],
        "",
        None,
        None,
        None,
        None,
        None,
        crate::ui::grid::TabGridContext {
            result_model: Some(model.clone().upcast()),
            ..Default::default()
        },
        None,
        std::sync::Arc::new(crate::services::database_service::DatabaseService::new_isolated()),
    );
    assert_eq!(selection.n_items(), 2);
    assert_eq!(view.model().unwrap().n_items(), 2);
    assert!(
        selection
            .item(0)
            .unwrap()
            .downcast::<RowObject>()
            .unwrap()
            .complete_cells()
            .is_none()
    );
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn result_grid_reports_rows_when_realized_and_released() {
    gtk4::init().unwrap();
    let model = PagedModel::new(&[2], 1);
    let columns = vec![bookie_core::ColumnInfo {
        name: "id".into(),
        data_type: "integer".into(),
        nullable: false,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }];
    let result = std::sync::Arc::new(bookie_core::QueryResult {
        columns: columns.clone(),
        rows: Vec::new().into(),
        truncated: false,
    });
    let visible = std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashSet::new()));
    let changes = visible.clone();
    let (view, _) = crate::ui::grid::build_column_view(
        &result,
        &columns,
        "",
        None,
        None,
        None,
        None,
        None,
        crate::ui::grid::TabGridContext {
            result_model: Some(model.upcast()),
            row_visibility: Some(std::rc::Rc::new(move |row, shown| {
                if shown {
                    changes.borrow_mut().insert(row);
                } else {
                    changes.borrow_mut().remove(&row);
                }
            })),
            ..Default::default()
        },
        None,
        std::sync::Arc::new(crate::services::database_service::DatabaseService::new_isolated()),
    );
    let window = gtk4::Window::builder().child(&view).build();
    window.present();
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while visible.borrow().is_empty() && std::time::Instant::now() < deadline {
        context.iteration(false);
    }
    assert!(!visible.borrow().is_empty());
    window.set_child(gtk4::Widget::NONE);
    while context.iteration(false) {}
    assert!(visible.borrow().is_empty());
    window.destroy();
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn realized_grid_rows_load_their_spooled_values() {
    gtk4::init().unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let spool = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default()
        .create()
        .unwrap();
    let column = bookie_core::ColumnInfo {
        name: "id".into(),
        data_type: "integer".into(),
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
    let result = std::sync::Arc::new(bookie_core::QueryResult {
        columns: vec![column.clone()],
        rows: vec![vec![bookie_core::Value::Int(42)]].into(),
        truncated: false,
    });
    runtime.block_on(spool.append_page(0, result.clone())).unwrap();
    let model = PagedModel::new(&[1], 1);
    let visibility = model.visibility_callback(runtime.handle(), &spool, std::slice::from_ref(&column));
    let empty = std::sync::Arc::new(bookie_core::QueryResult {
        columns: vec![column.clone()],
        rows: Vec::new().into(),
        truncated: false,
    });
    let (view, selection) = crate::ui::grid::build_column_view(
        &empty,
        &[column],
        "",
        None,
        None,
        None,
        None,
        None,
        crate::ui::grid::TabGridContext {
            result_model: Some(model.clone().upcast()),
            row_visibility: Some(visibility),
            ..Default::default()
        },
        None,
        std::sync::Arc::new(crate::services::database_service::DatabaseService::new_isolated()),
    );
    model.imp().selection.set(Some(&selection));
    let window = gtk4::Window::builder().child(&view).build();
    window.present();
    wait_for_first_value(&model);
    window.destroy();
}

#[cfg(test)]
fn wait_for_first_value(model: &PagedModel) {
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        while context.iteration(false) {}
        let row = model.item(0).unwrap().downcast::<RowObject>().unwrap();
        if row.complete_cells() == Some(vec![bookie_core::Value::Int(42)]) {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
