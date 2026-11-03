use super::*;

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
fn visible_rows_on_both_sides_of_a_page_boundary_remain_readable() {
    let model = PagedModel::with_limit(&[1, 1], 1, 4);
    let page = |value: &str| {
        std::sync::Arc::new(bookie_core::QueryResult {
            columns: Vec::new(),
            rows: vec![vec![bookie_core::Value::Text(value.into())]].into(),
            truncated: false,
        })
    };
    model.show(0, 2);
    assert_eq!(model.next_request(), Some(0));
    assert_eq!(model.next_request(), Some(1));
    assert!(model.install(0, page("first")));
    assert!(model.install(1, page("second")));
    for (position, expected) in [(0, "first"), (1, "second")] {
        let row = model.item(position).unwrap().downcast::<RowObject>().unwrap();
        assert_eq!(
            row.complete_cells(),
            Some(vec![bookie_core::Value::Text(expected.into())])
        );
    }
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn too_many_wide_visible_pages_show_a_fallback_and_clear_it_after_scrolling() {
    gtk4::init().unwrap();
    let model = PagedModel::with_limit(&[1, 1, 1], 1, 4);
    let status = gtk4::Label::new(None);
    model.attach_status(&status);
    model.show(0, 3);
    for value in ["first", "second", "third"] {
        let index = model.next_request().unwrap();
        let page = std::sync::Arc::new(bookie_core::QueryResult {
            columns: Vec::new(),
            rows: vec![vec![bookie_core::Value::Text(value.into())]].into(),
            truncated: false,
        });
        assert!(model.install(index, page));
    }
    assert!(status.text().contains("page view"));
    model.show(2, 1);
    assert!(status.text().is_empty());
}

#[test]
#[ignore = "requires an isolated GTK display"]
fn a_failed_offscreen_page_does_not_leave_a_stale_error_message() {
    gtk4::init().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let spool = bookie_app_services::stream_result_spool::StreamResultSpoolManager::default()
        .create()
        .unwrap();
    let model = PagedModel::new(&[1, 1], 1);
    let status = gtk4::Label::new(None);
    model.attach_status(&status);
    model.show(0, 1);
    model.load_from_spool(runtime.handle(), &spool, &[]);
    wait_for_condition(|| !status.text().is_empty());

    model.show(1, 1);
    assert!(status.text().is_empty());
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
    for (index, value) in [42, 84].into_iter().enumerate() {
        let result = std::sync::Arc::new(bookie_core::QueryResult {
            columns: vec![column.clone()],
            rows: vec![vec![bookie_core::Value::Int(value)]].into(),
            truncated: false,
        });
        runtime.block_on(spool.append_page(index as u64, result)).unwrap();
    }
    let model = PagedModel::with_limit(&[1, 1], 1, 1);
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
    wait_for_condition(|| has_label(view.upcast_ref(), "42") && has_label(view.upcast_ref(), "84"));
    window.destroy();
}

#[cfg(test)]
fn has_label(root: &gtk4::Widget, expected: &str) -> bool {
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        if widget
            .downcast_ref::<gtk4::Label>()
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
