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
    fn new(counts: &[usize], width: usize) -> Self {
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

    fn install(&self, index: u64, page: std::sync::Arc<bookie_core::QueryResult>) -> bool {
        let Some((start, count)) = self.imp().index.borrow().page_range(index) else {
            return false;
        };
        if count != page.rows.len() || page.rows.iter().any(|row| row.len() != self.imp().width.get()) {
            if let Some(window) = self.imp().pages.borrow_mut().as_mut() {
                window.failed(index);
            }
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
