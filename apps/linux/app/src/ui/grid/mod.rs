mod cell_context;
mod choice_editor;
mod close_edits;
pub(crate) use close_edits::finish_active_edits;
mod column;
mod column_jump;
mod column_order;
mod column_visibility;
#[cfg(test)]
mod column_width_tests;
mod context_menu;
mod dialog_focus;
mod display;
mod editing;
mod export;
mod font_zoom;
pub(crate) mod full_value_editor;
mod highlight_paint;
#[cfg(test)]
mod lifetime_tests;
pub(crate) mod local_filter;
mod local_sort;
mod presentation;
mod row_find;
mod types;
mod value_viewer;
pub(crate) use column_order::displayed_column_order;
pub(crate) use presentation::{cell_allows_inline_edit, cell_allows_set_null};

use gtk4::prelude::*;

use bookie_core::{ColumnInfo, QueryResult};

use crate::services::database_service::DatabaseService;
use crate::ui::row_store::RowStore;

use column::{ColumnSpec, build_column, header_column, is_cell_editable};
use context_menu::install_grid_context_menus;

pub(crate) use context_menu::{LoadedCellEdit, present_loaded_cell_editor};
pub(crate) use display::focused_row_position;
pub(crate) use display::preview_to_display_text;
pub use display::{focused_cell_identity, last_clicked_cell, set_null_display, value_to_display_text};
pub(crate) use value_viewer::present as present_value_viewer;

pub(crate) fn enter_cell_edit_mode(label: &crate::ui::cell_editor::CellEditor) {
    editing::enter_edit_mode(label);
}

#[derive(Debug)]
pub enum GridMsg {
    SortChanged(usize, bool),
    EditsFlushed {
        revision: u64,
        reply: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    CellEdited {
        row_position: u32,
        col_index: usize,
        new_value: String,
        /// Primary-key values read when the editor opened. The receiver
        /// rejects the edit if the row now at `row_position` is a
        /// different one.
        row_key: Vec<bookie_core::Value>,
    },
    CellEditedValue {
        row_position: u32,
        col_index: usize,
        value: bookie_core::Value,
        row_key: Vec<bookie_core::Value>,
    },
    CopyToClipboard(String),
    FetchCellValue {
        col_index: usize,
        column_name: String,
        row_key: Vec<bookie_core::Value>,
        edit_row_position: Option<u32>,
    },
    IncompleteRowData,
    ProjectionFailure,
    CopyRowAsStatement(Vec<u32>, bookie_core::sql_literal::RowStatement),
    ShowRowAsJson(String),
    ExportResults(std::sync::Arc<QueryResult>),
    ExportStreamedResults(crate::ui::export_dialog::StreamExportRequest),
    SetCellNull {
        row_position: u32,
        col_index: usize,
        row_key: Vec<bookie_core::Value>,
    },
    DeleteRowAt {
        row_position: u32,
        row_key: Vec<bookie_core::Value>,
    },
    InsertRow,
    DuplicateRow {
        row_position: u32,
    },
    FilterByValue {
        column: String,
        value: bookie_core::Value,
        exclude: bool,
    },
    FilterWholeResult {
        identity: crate::ui::editor::StatementIdentity,
        column: usize,
        value: bookie_core::Value,
        exclude: bool,
    },
    ShowToast(String),
    GoToReference {
        column: String,
        row: Vec<(String, bookie_core::Value)>,
    },
    ShowReferencingRows {
        row: Vec<(String, bookie_core::Value)>,
    },
}

#[derive(Clone, Default)]
pub struct TabGridContext {
    pub(crate) change_trackers: Option<crate::services::change_tracker::WorkspaceChangeTrackers>,
    pub tab_id: Option<uuid::Uuid>,
    pub pk_col_indices: Vec<usize>,
    pub projected_columns: Option<Vec<usize>>,
    pub preview_long_values: bool,
    pub preview_redis_strings: bool,
    pub preview_result_values: bool,
    pub local_result_sort: bool,
    pub row_mutations_allowed: bool,
    pub local_result_view: Option<local_filter::LocalResultView>,
    pub whole_result_filter: bool,
    pub changed_cells: Option<std::sync::Arc<bookie_app_services::result_compare::ChangedCells>>,
    pub highlight_rules: Option<std::sync::Arc<[bookie_app_services::highlight_match::PreparedHighlightRule]>>,
    pub(crate) result_model: Option<gtk4::gio::ListModel>,
    pub(crate) row_visibility: Option<std::rc::Rc<dyn Fn(u32, bool)>>,
    pub sortable_result_columns: std::collections::HashSet<usize>,
    /// Names of columns that are part of a foreign key on this table.
    /// Marks the column header so a reference is visible before the
    /// cell value picker (a later slice) exists.
    pub foreign_key_columns: std::collections::HashSet<String>,
    pub has_inbound_foreign_keys: bool,
    pub filters_unavailable: bool,
    pub choices: ChoiceMap,
    pub ambiguous_keys: AmbiguousKeys,
}

pub type AmbiguousKeys =
    std::rc::Rc<std::cell::RefCell<std::collections::HashSet<crate::services::change_tracker::RowKey>>>;

pub type ChoiceMap = std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, (bool, Vec<String>)>>>;

const WIDE_TABLE_THRESHOLD: usize = 8;
const MIN_COLUMN_WIDTH_PX: i32 = 120;

pub struct GridSenders {
    pub edit: Option<relm4::Sender<GridMsg>>,
    pub menu: Option<relm4::Sender<GridMsg>>,
    pub sort: Option<relm4::Sender<GridMsg>>,
}

pub struct GridEnv {
    pub connection_id: Option<uuid::Uuid>,
    pub tab_ctx: TabGridContext,
    pub column_widths: Option<crate::services::column_widths::ColumnWidthStore>,
    pub database: std::sync::Arc<DatabaseService>,
}

pub fn build_column_view(
    shared: &std::sync::Arc<QueryResult>,
    schema_columns: &[ColumnInfo],
    table: &str,
    sort: Option<(usize, bool)>,
    senders: GridSenders,
    env: GridEnv,
) -> (gtk4::ColumnView, gtk4::MultiSelection) {
    let GridSenders {
        edit: edit_sender,
        menu: menu_sender,
        sort: sort_sender,
    } = senders;
    let GridEnv {
        connection_id,
        tab_ctx,
        column_widths,
        database,
    } = env;
    let result: &QueryResult = shared;
    let store = match &tab_ctx.projected_columns {
        Some(indices) => match if tab_ctx.preview_redis_strings && !tab_ctx.pk_col_indices.is_empty() {
            RowStore::from_projected_with_redis_string_previews(
                shared.clone(),
                indices.clone(),
                schema_columns.len(),
                tab_ctx.pk_col_indices.clone(),
            )
        } else if tab_ctx.preview_long_values && !tab_ctx.pk_col_indices.is_empty() {
            RowStore::from_projected_with_previews(
                shared.clone(),
                indices.clone(),
                schema_columns.len(),
                tab_ctx.pk_col_indices.clone(),
            )
        } else {
            RowStore::from_projected(shared.clone(), indices.clone(), schema_columns.len())
        } {
            Ok(store) => store,
            Err(error) => {
                tracing::error!(%error, "projected browse rows do not match the table schema");
                if let Some(sender) = &menu_sender {
                    sender.send(GridMsg::ProjectionFailure).ok();
                }
                RowStore::from_shared(std::sync::Arc::new(QueryResult {
                    columns: schema_columns.to_vec(),
                    rows: Vec::new().into(),
                    truncated: false,
                }))
            }
        },
        None if tab_ctx.preview_redis_strings && !tab_ctx.pk_col_indices.is_empty() => {
            RowStore::from_shared_with_redis_string_previews(shared.clone(), tab_ctx.pk_col_indices.clone())
        }
        None if tab_ctx.preview_result_values => RowStore::from_shared_with_result_previews(shared.clone()),
        None if tab_ctx.preview_long_values && !tab_ctx.pk_col_indices.is_empty() => {
            RowStore::from_shared_with_previews(shared.clone(), tab_ctx.pk_col_indices.clone())
        }
        None => RowStore::from_shared(shared.clone()),
    };
    let columns = if tab_ctx.projected_columns.is_some() {
        schema_columns
    } else {
        &result.columns
    };
    let model = tab_ctx.result_model.clone().unwrap_or_else(|| store.clone().upcast());
    let selection = gtk4::MultiSelection::new(Some(model));
    if tab_ctx.result_model.is_none()
        && let Some(local_result_view) = &tab_ctx.local_result_view
    {
        local_result_view.attach(store.clone(), selection.clone());
    }
    let column_view = gtk4::ColumnView::builder()
        .model(&selection)
        .show_row_separators(true)
        .show_column_separators(true)
        .build();

    if let Some(sender) = menu_sender.as_ref() {
        let driver_id = connection_id
            .and_then(|id| database.metadata(id))
            .map(|metadata| metadata.driver_id)
            .unwrap_or_default();
        install_grid_context_menus(
            &column_view,
            sender.clone(),
            columns,
            result.truncated,
            driver_id,
            context_menu::GridMenuOptions {
                filterable: tab_ctx.tab_id.is_some() && !tab_ctx.filters_unavailable,
                row_mutations_allowed: tab_ctx.row_mutations_allowed,
                foreign_key_columns: tab_ctx.foreign_key_columns.clone(),
                has_inbound_foreign_keys: tab_ctx.has_inbound_foreign_keys,
                local_result_view: tab_ctx.local_result_view.clone(),
                whole_result_filter: tab_ctx.whole_result_filter,
                snapshot_export: tab_ctx.result_model.is_none(),
            },
        );
    }

    let default_min_width = if columns.len() > WIDE_TABLE_THRESHOLD {
        Some(MIN_COLUMN_WIDTH_PX)
    } else {
        None
    };
    let mut view_columns: Vec<gtk4::ColumnViewColumn> = Vec::with_capacity(columns.len());
    let mut edit_triggers = Vec::with_capacity(columns.len());
    for (i, column) in columns.iter().enumerate() {
        let editable = is_cell_editable(schema_columns.get(i).unwrap_or(column));
        let header = header_column(column, schema_columns.get(i));
        let spec = ColumnSpec {
            editable,
            table: table.to_string(),
            sender: edit_sender.clone(),
            connection_id,
            tab_ctx: tab_ctx.clone(),
            sort_enabled: sort_sender.is_some()
                || (tab_ctx.local_result_sort && tab_ctx.sortable_result_columns.contains(&i)),
            default_min_width,
            column_widths: column_widths.clone(),
        };
        let (col, edit_trigger) = build_column(&header, i, spec);
        col.set_id(Some(&i.to_string()));
        column_view.append_column(&col);
        view_columns.push(col);
        edit_triggers.push(edit_trigger);
    }
    editing::install_grid_edit_triggers(&column_view, edit_triggers);

    let grid_actions = gtk4::gio::SimpleActionGroup::new();
    column_visibility::install(
        &column_view,
        &grid_actions,
        &view_columns,
        columns.iter().map(|column| column.name.clone()).collect(),
        connection_id
            .zip(column_widths.clone())
            .map(|(id, store)| (id, table.to_string(), store)),
    );
    column_order::install(
        &column_view,
        &view_columns,
        columns.iter().map(|column| column.name.clone()).collect(),
        connection_id
            .zip(column_widths.clone())
            .map(|(id, store)| (id, table.to_string(), store)),
    );
    column_jump::install(
        &column_view,
        &grid_actions,
        &view_columns,
        columns.iter().map(|column| column.name.clone()).collect(),
        connection_id,
        database,
    );
    if tab_ctx.result_model.is_none() {
        row_find::install(
            &column_view,
            &grid_actions,
            columns.iter().map(|column| column.name.clone()).collect(),
        );
    }
    font_zoom::install(&column_view, &grid_actions);
    column_view.insert_action_group("grid", Some(&grid_actions));

    if let Some((col_idx, ascending)) = sort
        && let Some(col) = view_columns.get(col_idx)
    {
        let direction = if ascending {
            gtk4::SortType::Ascending
        } else {
            gtk4::SortType::Descending
        };
        column_view.sort_by_column(Some(col), direction);
    }

    if let Some(view_sorter) = column_view
        .sorter()
        .and_then(|sorter| sorter.downcast::<gtk4::ColumnViewSorter>().ok())
        && (sort_sender.is_some() || tab_ctx.local_result_sort)
    {
        let columns_for_sort = view_columns.clone();
        let local = tab_ctx.local_result_sort;
        let store_for_sort = store.clone();
        let selection_for_sort = selection.clone();
        let result_for_sort = shared.clone();
        let local_result_view = tab_ctx.local_result_view.clone();
        let sender_for_sort = sort_sender.clone();
        let dispatch = move |sorter: &gtk4::ColumnViewSorter| {
            let Some(active) = sorter.primary_sort_column() else {
                return;
            };
            let ascending = matches!(sorter.primary_sort_order(), gtk4::SortType::Ascending);
            let Some(index) = columns_for_sort.iter().position(|column| column == &active) else {
                return;
            };
            if local {
                if let Some(view) = &local_result_view {
                    view.sort_by(index, ascending);
                } else {
                    sort_local_result(&result_for_sort, index, ascending, &store_for_sort, &selection_for_sort);
                }
            } else if let Some(sender) = &sender_for_sort {
                sender.send(GridMsg::SortChanged(index, ascending)).ok();
            }
        };
        view_sorter.connect_primary_sort_column_notify({
            let dispatch = dispatch.clone();
            move |sorter| dispatch(sorter)
        });
        view_sorter.connect_primary_sort_order_notify(move |sorter| dispatch(sorter));
    }

    (column_view, selection)
}

fn sort_local_result(
    result: &std::sync::Arc<QueryResult>,
    column: usize,
    ascending: bool,
    store: &RowStore,
    selection: &gtk4::MultiSelection,
) {
    let Some(order) = local_sort::sorted_source_indices(result, column, ascending) else {
        return;
    };
    let selected_sources = (0..selection.n_items())
        .filter(|position| selection.is_selected(*position))
        .filter_map(|position| store.source_index_at(position))
        .collect::<std::collections::HashSet<_>>();
    if !store.reorder_source_rows(&order) {
        return;
    }
    let positions = gtk4::Bitset::new_empty();
    for (position, source_index) in order.iter().enumerate() {
        if selected_sources.contains(source_index) {
            positions.add(position as u32);
        }
    }
    selection.set_selection(&positions, &gtk4::Bitset::new_range(0, order.len() as u32));
}

pub(crate) fn sortable_result_columns(result: &QueryResult) -> std::collections::HashSet<usize> {
    local_sort::sortable_columns(result)
}

pub(crate) fn jump_to_column(widget: &gtk4::Widget) -> bool {
    column_jump::open_in(widget)
}

#[cfg(test)]
mod local_sort_tests {
    use super::*;

    #[test]
    fn sortable_columns_preserve_null_mixed_type_and_missing_cell_rules() {
        let columns = (0..5)
            .map(|index| ColumnInfo {
                name: format!("column_{index}"),
                data_type: "INTEGER".into(),
                nullable: true,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
                comment: None,
                collation: None,
                enum_type: None,
                domain_type: None,
            })
            .collect();
        let result = QueryResult {
            columns,
            rows: vec![
                vec![
                    bookie_core::Value::Int(1),
                    bookie_core::Value::Null,
                    bookie_core::Value::Int(3),
                    bookie_core::Value::Int(4),
                    bookie_core::Value::Int(5),
                ],
                vec![
                    bookie_core::Value::Null,
                    bookie_core::Value::Null,
                    bookie_core::Value::Text("3".into()),
                    bookie_core::Value::Undecodable("unknown".into()),
                ],
            ]
            .into(),
            truncated: false,
        };

        let expected = std::collections::HashSet::from([0, 1]);
        assert_eq!(sortable_result_columns(&result), expected);
        for column in 0..result.columns.len() {
            assert_eq!(
                local_sort::is_sortable_column(&result, column),
                expected.contains(&column)
            );
        }
        let empty = QueryResult {
            columns: result.columns.clone(),
            rows: Vec::new().into(),
            truncated: false,
        };
        assert_eq!(sortable_result_columns(&empty), (0..result.columns.len()).collect());
    }

    #[test]
    #[ignore = "requires isolated GTK display"]
    fn local_sort_keeps_selection_attached_to_its_source_row() {
        gtk4::init().unwrap();
        let result = std::sync::Arc::new(QueryResult {
            columns: vec![bookie_core::ColumnInfo {
                name: "value".into(),
                data_type: "INTEGER".into(),
                nullable: false,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
                comment: None,
                collation: None,
                enum_type: None,
                domain_type: None,
            }],
            rows: vec![
                vec![bookie_core::Value::Int(3)],
                vec![bookie_core::Value::Int(1)],
                vec![bookie_core::Value::Int(2)],
            ]
            .into(),
            truncated: false,
        });
        let store = RowStore::from_shared(result.clone());
        let selection = gtk4::MultiSelection::new(Some(store.clone()));
        selection.select_item(0, true);
        selection.select_item(2, false);

        sort_local_result(&result, 0, true, &store, &selection);

        let positions = selection.selection();
        assert_eq!(positions.size(), 2);
        assert_eq!(store.source_index_at(positions.nth(0)), Some(2));
        assert_eq!(store.source_index_at(positions.nth(1)), Some(0));
        assert_eq!(
            store.source_value_at(positions.nth(1), 0),
            Some(bookie_core::Value::Int(3))
        );
    }

    #[test]
    #[ignore = "requires isolated GTK display"]
    fn sorting_a_fully_selected_result_restores_the_selection_in_one_signal() {
        gtk4::init().unwrap();
        let rows = (0..2000_i64)
            .rev()
            .map(|value| vec![bookie_core::Value::Int(value)])
            .collect();
        let result = std::sync::Arc::new(QueryResult {
            columns: vec![bookie_core::ColumnInfo {
                name: "value".into(),
                data_type: "INTEGER".into(),
                nullable: false,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
                comment: None,
                collation: None,
                enum_type: None,
                domain_type: None,
            }],
            rows,
            truncated: false,
        });
        let store = RowStore::from_shared(result.clone());
        let selection = gtk4::MultiSelection::new(Some(store.clone()));
        selection.select_all();
        let emissions = std::rc::Rc::new(std::cell::Cell::new(0_u32));
        let counter = emissions.clone();
        selection.connect_selection_changed(move |_, _, _| counter.set(counter.get() + 1));

        sort_local_result(&result, 0, true, &store, &selection);

        assert_eq!(selection.selection().size(), 2000);
        assert!(emissions.get() <= 2, "{} selection-changed emissions", emissions.get());
    }
}
