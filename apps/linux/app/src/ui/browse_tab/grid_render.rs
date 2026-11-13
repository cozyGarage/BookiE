use relm4::gtk::glib;
use relm4::{ComponentSender, gtk};

use bookie_core::QueryResult;

use super::*;

pub(super) fn column_layout_matches(rendered: &[ColumnInfo], current: &[ColumnInfo]) -> bool {
    rendered == current
}

impl BrowseTab {
    pub(super) fn list_store(&self) -> Option<crate::ui::row_store::RowStore> {
        let selection = self.current_selection.as_ref()?;
        selection.model()?.downcast::<crate::ui::row_store::RowStore>().ok()
    }

    pub(super) fn refresh_row_at(&self, pos: u32) {
        let row_obj = self.row_object_at(pos);
        let store = self.list_store();
        let store_pos = match (row_obj.as_ref(), store.as_ref()) {
            (Some(r), Some(s)) => s.find(r),
            _ => None,
        };
        // ColumnView skips rebinding when a changed item keeps the same GObject identity.
        if let (Some(store), Some(store_pos), Some(old)) = (store, store_pos, row_obj) {
            let replacement = old.clone_preserving_loading_state();
            store.splice(store_pos, 1, &[replacement]);
        }
    }

    pub(super) fn row_object_at(&self, position: u32) -> Option<crate::ui::row_object::RowObject> {
        let model = self.current_selection.as_ref()?.model()?;
        model
            .item(position)?
            .downcast::<crate::ui::row_object::RowObject>()
            .ok()
    }

    pub(super) fn capture_focus_for_restore(&self) {
        use crate::services::change_tracker::RowKey;
        // A second reload must not replace the focus captured before the first finishes.
        if self.pending_focus_restore.borrow().is_some() {
            return;
        }
        let Some(selection) = self.current_selection.as_ref() else {
            return;
        };
        let bitset = selection.selection();
        if bitset.size() == 0 {
            return;
        }
        let pos = bitset.nth(0);
        let Some(model) = selection.model() else { return };
        let Some(item) = model.item(pos) else { return };
        let Ok(row) = item.downcast::<crate::ui::row_object::RowObject>() else {
            return;
        };
        let key = if let Some(draft_id) = row.draft_id() {
            Some(RowKey::Draft(draft_id))
        } else {
            let pk_indices: Vec<usize> = self
                .current_columns
                .iter()
                .enumerate()
                .filter(|(_, c)| c.primary_key)
                .map(|(i, _)| i)
                .collect();
            if pk_indices.is_empty() {
                None
            } else {
                let pk_values: Vec<Value> = pk_indices.iter().map(|&i| row.cell_value(i)).collect();
                RowKey::from_pk_values(&pk_values)
            }
        };
        *self.pending_focus_restore.borrow_mut() = key;
    }

    pub(super) fn focus_inserted_draft(&self) {
        let Some(cv) = self.current_column_view.clone() else {
            return;
        };
        let selection = self.current_selection.clone();
        glib::idle_add_local_once(move || {
            // The deferred focus can outlive the view after a refresh.
            if cv.root().is_none() {
                return;
            }
            if let Some(sel) = selection.as_ref() {
                sel.select_item(0, true);
            }
            cv.scroll_to(
                0,
                None,
                gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT,
                None,
            );
            let Some(window) = cv.root().and_then(|r| r.dynamic_cast::<gtk::Window>().ok()) else {
                return;
            };
            let Some(focused) = gtk::prelude::GtkWindowExt::focus(&window) else {
                return;
            };
            if let Ok(label) = focused.dynamic_cast::<crate::ui::cell_editor::CellEditor>() {
                crate::ui::grid::enter_cell_edit_mode(&label);
            }
        });
    }

    pub(super) fn restore_focused_row(&self) {
        let Some(key) = self.pending_focus_restore.borrow_mut().take() else {
            return;
        };
        let Some(position) = self.find_row_position_by_key(&key) else {
            return;
        };
        if let Some(selection) = self.current_selection.as_ref() {
            selection.select_item(position, true);
        }
        if let Some(cv) = self.current_column_view.as_ref() {
            cv.scroll_to(position, None, gtk::ListScrollFlags::FOCUS, None);
        }
    }

    fn refresh_ambiguous_keys(&self, result: &bookie_core::QueryResult) {
        let ambiguous = if self.driver_id == "clickhouse" {
            let key_indices: Vec<usize> = self
                .current_columns
                .iter()
                .enumerate()
                .filter(|(_, column)| column.primary_key)
                .map(|(index, _)| index)
                .collect();
            crate::services::change_tracker::ambiguous_row_keys(&result.rows, &key_indices)
        } else {
            Default::default()
        };
        *self.ambiguous_keys.borrow_mut() = ambiguous;
    }

    fn prepared_highlight_rules(
        &self,
    ) -> Option<std::sync::Arc<[bookie_app_services::highlight_match::PreparedHighlightRule]>> {
        let connection_id = self.connection_id?;
        let store = self.persistence.highlight_settings.as_ref()?;
        let columns: Vec<&str> = self.current_columns.iter().map(|column| column.name.as_str()).collect();
        store.prepared(connection_id, self.schema.as_deref(), &self.table, &columns)
    }

    fn highlight_persist(&self) -> Option<crate::ui::grid::HighlightPersist> {
        let connection_id = self.connection_id?;
        let store = self.persistence.highlight_settings.clone()?;
        Some(crate::ui::grid::HighlightPersist {
            connection_id,
            schema: self.schema.clone(),
            table: self.table.clone(),
            store,
        })
    }

    pub(super) fn reload_highlight_rules(&mut self, sender: ComponentSender<Self>) {
        self.current_column_view = None;
        self.render_grid_if_ready(sender);
    }

    pub(super) fn render_grid_if_ready(&mut self, sender: ComponentSender<Self>) {
        let Some(result) = self.current_result.clone() else {
            return;
        };
        if self.current_columns.is_empty() {
            return;
        }
        self.refresh_ambiguous_keys(&result);
        let dropped_column_selection = self.drop_column_selection();

        // Reuse the view to avoid rebuilding each column factory after page updates.
        if self.column_view_matches_current_columns()
            && let Some(store) = self.list_store()
        {
            if !self.refresh_grid_data(&result, &store) {
                self.show_projection_error(&sender);
                return;
            }
            self.refresh_grid_chrome();
            self.restore_focused_row();
            if dropped_column_selection {
                self.refresh_selection_chrome();
            }
            let _ = sender.output(BrowseTabOutput::StateChanged);
            return;
        }

        clear_box(&self.grid_holder);
        let edit_sender = if self.allows_keyed_row_mutations() {
            Some(self.grid_sender.clone())
        } else {
            None
        };
        let pk_col_indices: Vec<usize> = self
            .current_columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.primary_key)
            .map(|(i, _)| i)
            .collect();
        let foreign_key_columns = self
            .foreign_keys
            .iter()
            .flat_map(|fk| fk.columns.iter().cloned())
            .collect();
        let tab_ctx = TabGridContext {
            change_trackers: Some(self.change_trackers.clone()),
            tab_id: Some(self.tab_id),
            pk_col_indices,
            projected_columns: self.current_projection.clone(),
            preview_long_values: self.driver_id != "redis",
            preview_redis_strings: self.driver_id == "redis",
            preview_result_values: false,
            local_result_sort: false,
            row_mutations_allowed: self.allows_keyed_row_mutations(),
            local_result_view: None,
            whole_result_filter: false,
            changed_cells: None,
            result_model: None,
            row_visibility: None,
            sortable_result_columns: Default::default(),
            highlight_rules: self.prepared_highlight_rules(),
            foreign_key_columns,
            has_inbound_foreign_keys: !self.inbound_foreign_keys.is_empty(),
            filters_unavailable: !self.browse_filters,
            choices: self.choice_labels.clone(),
            ambiguous_keys: self.ambiguous_keys.clone(),
            row_number_base: self.current_offset,
            column_selection: self.column_selection.clone(),
            set_cells_enabled: self.allows_keyed_row_mutations(),
            column_selection_chrome: None,
        };
        let (column_view, selection) = build_column_view(
            &result,
            &self.current_columns,
            &self.table,
            self.current_sort,
            crate::ui::grid::GridSenders {
                edit: edit_sender,
                menu: Some(self.grid_sender.clone()),
                sort: Some(self.grid_sender.clone()),
            },
            crate::ui::grid::GridEnv {
                connection_id: self.connection_id,
                tab_ctx,
                column_widths: self.persistence.column_widths.clone(),
                highlight_settings: self.highlight_persist(),
                database: self.database.clone(),
            },
        );
        self.current_selection = Some(selection);
        self.current_column_view = Some(column_view.clone());
        self.rendered_columns.clone_from(&self.current_columns);

        let selection_label_for_signal = self.selection_label.clone();
        let selection_count_label_for_signal = self.selection_count_label.clone();
        let selection_details_label_for_signal = self.selection_details_label.clone();
        if let Some(sel) = self.current_selection.as_ref() {
            let inspector = self.inspector.downgrade();
            let columns = self.current_columns.clone();
            let deletable = self.allows_keyed_row_mutations();
            sel.connect_selection_changed(move |sel, _, _| {
                let n = sel.selection().size() as u32;
                let tooltip = selection_tooltip(sel, &columns);
                update_selection_chrome(
                    &selection_label_for_signal,
                    &selection_count_label_for_signal,
                    &selection_details_label_for_signal,
                    n,
                    tooltip.as_deref(),
                    deletable,
                );
                inspector.show_row(inspected_row(sel, &columns));
            });
            // Rebuilding the page clears the selection bitset.
            update_selection_chrome(
                &self.selection_label,
                &self.selection_count_label,
                &self.selection_details_label,
                0,
                None,
                false,
            );
        }

        // The tracker retains draft rows while RowsLoaded replaces the model.
        self.reprepend_drafts();

        let scrolled = crate::ui::grid::wrap_grid_scrolled(&column_view);
        self.grid_holder.append(&scrolled);
        self.apply_local_sort_keys();
        self.update_sort_status_label();
        self.refresh_grid_chrome();
        self.restore_focused_row();
        let _ = sender.output(BrowseTabOutput::StateChanged);
    }

    pub(super) fn column_view_matches_current_columns(&self) -> bool {
        self.current_column_view.is_some() && column_layout_matches(&self.rendered_columns, &self.current_columns)
    }

    pub(super) fn refresh_grid_data(
        &self,
        result: &std::sync::Arc<QueryResult>,
        store: &crate::ui::row_store::RowStore,
    ) -> bool {
        let key_indices = self
            .current_columns
            .iter()
            .enumerate()
            .filter_map(|(index, column)| column.primary_key.then_some(index))
            .collect();
        if let Some(indices) = &self.current_projection {
            let replaced = if self.driver_id == "redis" {
                store.replace_projected_with_redis_string_previews(
                    result.clone(),
                    indices.clone(),
                    self.current_columns.len(),
                    key_indices,
                )
            } else {
                store.replace_projected_with_previews(
                    result.clone(),
                    indices.clone(),
                    self.current_columns.len(),
                    key_indices,
                )
            };
            if let Err(error) = replaced {
                tracing::error!(%error, "projected browse refresh does not match the table schema");
                store.replace_shared(std::sync::Arc::new(QueryResult {
                    columns: self.current_columns.clone(),
                    rows: Vec::new().into(),
                    truncated: false,
                }));
                return false;
            }
        } else {
            if self.driver_id == "redis" {
                store.replace_shared_with_redis_string_previews(result.clone(), key_indices);
            } else {
                store.replace_shared_with_previews(result.clone(), key_indices);
            }
        }
        self.reprepend_drafts();
        true
    }

    pub(super) fn show_projection_error(&self, sender: &ComponentSender<Self>) {
        self.show_error_inner(
            &bookie_app_services::tr!("The database returned rows that do not match the table columns."),
            sender,
        );
        self.inner_stack.set_visible_child_name("error");
    }

    pub(super) fn refresh_grid_chrome(&self) {
        self.refresh_crud_buttons();
        self.update_paginator_label();
        let on_first_page = self.current_offset == 0;
        self.first_button.set_sensitive(!on_first_page);
        self.prev_button.set_sensitive(!on_first_page);
        self.update_next_page_sensitivity();
        // The last-page target is unavailable until the row count arrives.
        let last_target = self
            .current_total_rows
            .filter(|t| *t > 0)
            .map(|t| (t - 1) / self.page_size * self.page_size);
        self.last_button
            .set_sensitive(last_target.is_some_and(|target| self.current_offset != target));

        self.refresh_inner_stack_visibility();
    }

    /// Keep empty results in the built grid; switching to a hidden child leaves GTK adjustments unallocated.
    /// Scrolling to the first draft then triggers `gtk_list_base_update_adjustments`.
    pub(super) fn refresh_inner_stack_visibility(&self) {
        if self.current_result.is_some() {
            self.inner_stack.set_visible_child_name("grid");
        }
    }

    pub(super) fn reprepend_drafts(&self) {
        let Some(store) = self.list_store() else {
            return;
        };
        let drafts = self
            .change_trackers
            .with_tab_ref(self.tab_id, |t| {
                t.drafts()
                    .iter()
                    .map(|d| (d.draft_id, d.values.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (draft_id, values) in drafts {
            let draft_row = crate::ui::row_object::RowObject::new_draft(draft_id, values);
            store.insert(0, &draft_row);
        }
    }

    /// Restore the canonical cell display after rejecting an invalid edit.
    pub(super) fn refresh_row(&self, position: u32) {
        self.refresh_row_at(position);
    }
}

fn inspected_row(
    selection: &gtk::MultiSelection,
    columns: &[ColumnInfo],
) -> Option<Vec<crate::ui::row_inspector::InspectorField>> {
    let bits = selection.selection();
    if bits.size() != 1 {
        return None;
    }
    let row = selection
        .model()?
        .item(bits.nth(0))?
        .downcast::<crate::ui::row_object::RowObject>()
        .ok()?;
    Some(crate::ui::row_inspector::inspector_fields_for_row(columns, &row))
}

pub(super) fn selection_tooltip(selection: &gtk::MultiSelection, columns: &[ColumnInfo]) -> Option<String> {
    let model = selection.model()?;
    let bits = selection.selection();
    let rows: Vec<crate::ui::row_object::RowObject> = (0..bits.size())
        .filter_map(|i| {
            model
                .item(bits.nth(i as u32))?
                .downcast::<crate::ui::row_object::RowObject>()
                .ok()
        })
        .collect();
    let stats = super::selection::stats::column_stats(columns, &rows);
    Some(super::selection::stats::tooltip(&stats))
}

#[cfg(test)]
mod selection_stats_tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn wide_selection_statistics_preserve_rows_and_selection() {
        adw::init().unwrap();
        let store = gtk::gio::ListStore::new::<crate::ui::row_object::RowObject>();
        for number in 0..64 {
            store.append(&crate::ui::row_object::RowObject::new(vec![
                bookie_core::Value::Int(number),
                bookie_core::Value::Text("x".repeat(512 * 1024)),
            ]));
        }
        let columns = ["number", "payload"].map(|name| ColumnInfo {
            name: name.into(),
            data_type: String::new(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        });
        let selection = gtk::MultiSelection::new(Some(store.clone()));
        selection.select_all();
        for _ in 0..8 {
            assert!(!selection_tooltip(&selection, &columns).unwrap().is_empty());
        }
        assert_eq!(selection.selection().size(), 64);
        let row = store
            .item(0)
            .unwrap()
            .downcast::<crate::ui::row_object::RowObject>()
            .unwrap();
        assert_eq!(
            row.with_cell_value(1, |value| match value {
                bookie_core::Value::Text(text) => text.len(),
                _ => 0,
            }),
            Some(512 * 1024)
        );
    }
}
