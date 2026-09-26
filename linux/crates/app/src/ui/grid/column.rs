use gtk4::prelude::*;

use tablepro_core::{ColumnInfo, Value};

use super::context_menu::GridMenus;
use super::display::{
    FULL_EDIT_TEXT_SLOT, POPOVER_SLOT, POSITION_SLOT, ROW_KEY_SLOT, SNAPSHOT_SLOT, SUPPRESS_SLOT, cell_text_for_bind,
    value_to_full_edit_text,
};
use super::editing::{setup_bool_cell, setup_editable_cell, setup_readonly_cell};
use super::presentation::cell_allows_inline_edit;
pub(super) use super::presentation::column_is_editable as is_cell_editable;
use super::types::{classify_editor_kind, is_bool_type};
use super::{GridMsg, TabGridContext};

const PENDING_CSS_CLASSES: &[&str] = &[
    "tp-cell-modified",
    "tp-row-pending-delete",
    "tp-row-pending-insert",
    "tp-row-leftmost-error-flash",
];

const TOOLTIP_MIN_CHARS: usize = 40;

/// Marks a foreign-key column's header before the cell value picker
/// (a later slice) exists to act on it.
fn column_header_title(name: &str, is_foreign_key: bool) -> String {
    if is_foreign_key {
        format!("{name} \u{1F517}")
    } else {
        name.to_string()
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_column(
    info: &ColumnInfo,
    idx: usize,
    editable: bool,
    table: String,
    sender: Option<relm4::Sender<GridMsg>>,
    sort_sender: Option<relm4::Sender<GridMsg>>,
    connection_id: Option<uuid::Uuid>,
    tab_ctx: TabGridContext,
    default_min_width: Option<i32>,
    column_view: gtk4::ColumnView,
    grid_menus: Option<GridMenus>,
    column_widths: Option<crate::services::column_widths::ColumnWidthStore>,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();
    let edit_sender = if editable { sender.clone() } else { None };
    let readonly_sender = sender.clone();
    let table_for_persist = table;

    let column_data_type = info.data_type.clone();
    let column_name = info.name.clone();
    let column_view_for_setup = column_view.clone();
    let grid_menus_for_setup = grid_menus.clone();
    factory.connect_setup(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        if let Some(edit_sender) = edit_sender.clone() {
            if is_bool_type(&column_data_type) {
                setup_bool_cell(
                    item,
                    idx,
                    column_name.clone(),
                    edit_sender,
                    &column_view_for_setup,
                    grid_menus_for_setup.as_ref(),
                );
            } else {
                let editor_kind = classify_editor_kind(&column_data_type);
                setup_editable_cell(
                    item,
                    idx,
                    column_name.clone(),
                    edit_sender,
                    editor_kind,
                    &column_view_for_setup,
                    grid_menus_for_setup.as_ref(),
                );
            }
        } else {
            setup_readonly_cell(
                item,
                idx,
                column_name.clone(),
                readonly_sender.clone(),
                &column_view_for_setup,
                grid_menus_for_setup.as_ref(),
            );
        }
    });

    let editable_for_bind = editable && sender.is_some();
    let column_info = info.clone();
    let column_auto_filled = info.is_auto_increment || info.is_generated;
    let tab_ctx_for_bind = tab_ctx.clone();
    factory.connect_bind(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(row) = item.item().and_downcast::<crate::ui::row_object::RowObject>() else {
            return;
        };
        let raw_value = row.cell_value(idx);
        let pk_values: Vec<Value> = tab_ctx_for_bind
            .pk_col_indices
            .iter()
            .map(|&i| row.cell_value(i))
            .collect();
        let value = if let Some(tab_id) = tab_ctx_for_bind.tab_id
            && row.draft_id().is_none()
        {
            crate::services::change_tracker::with_tab_ref(tab_id, |t| {
                crate::services::change_tracker::RowKey::from_pk_values(&pk_values)
                    .map(|key| t.current_cell_value(&key, idx, &raw_value).clone())
                    .unwrap_or_else(|| raw_value.clone())
            })
            .unwrap_or(raw_value)
        } else {
            raw_value
        };
        let is_null = matches!(value, Value::Null);
        let inline_editable = editable_for_bind && cell_allows_inline_edit(&column_info, &value);
        let text = cell_text_for_bind(&value, editable_for_bind, column_auto_filled);

        let pending_classes: Vec<&'static str> = if let Some(_tab_id) = tab_ctx_for_bind.tab_id {
            if row.draft_id().is_some() {
                vec!["tp-row-pending-insert"]
            } else {
                let pk_values: Vec<Value> = tab_ctx_for_bind
                    .pk_col_indices
                    .iter()
                    .map(|&i| row.cell_value(i))
                    .collect();
                crate::services::change_tracker::with_tab_ref(_tab_id, |t| {
                    let mut v: Vec<&'static str> = Vec::new();
                    let Some(key) = crate::services::change_tracker::RowKey::from_pk_values(&pk_values) else {
                        return v;
                    };
                    let row_state = t.row_state(&key);
                    let cell_state = t.cell_state(&key, idx);
                    use crate::services::change_tracker::{CellState, RowState};
                    match (row_state, cell_state) {
                        (RowState::PendingDelete, _) => v.push("tp-row-pending-delete"),
                        (RowState::InsertDraft, _) => v.push("tp-row-pending-insert"),
                        (_, CellState::Modified) => v.push("tp-cell-modified"),
                        _ => {}
                    }
                    if idx == 0 && t.is_error_row(&key) {
                        v.push("tp-row-leftmost-error-flash");
                    }
                    v
                })
                .unwrap_or_default()
            }
        } else {
            Vec::new()
        };

        let is_pending_delete = pending_classes.contains(&"tp-row-pending-delete");
        let Some(child) = item.child() else { return };
        if let Ok(label) = child.clone().downcast::<crate::ui::cell_editor::CellEditor>() {
            label.set_inline_editable(inline_editable);
            label.set_text(&text);
            apply_cell_tooltip(label.upcast_ref(), &text, is_null);
            if is_null && !editable_for_bind {
                label.add_css_class("dim-label");
            } else {
                label.remove_css_class("dim-label");
            }
            if is_null && (editable_for_bind || column_auto_filled) {
                label.add_css_class("tp-null-sentinel");
            } else {
                label.remove_css_class("tp-null-sentinel");
            }
            clear_pending_classes(label.upcast_ref());
            for cls in &pending_classes {
                label.add_css_class(cls);
            }
            label.set_strikethrough(is_pending_delete);
            POSITION_SLOT.set(&label, item.position());
            ROW_KEY_SLOT.set(&label, pk_values.clone());
            if inline_editable {
                FULL_EDIT_TEXT_SLOT.set(&label, value_to_full_edit_text(&value));
            } else {
                FULL_EDIT_TEXT_SLOT.take(&label);
            }
        } else if let Ok(checkbox) = child.clone().downcast::<gtk4::CheckButton>() {
            checkbox.set_sensitive(inline_editable);
            SUPPRESS_SLOT.set(&checkbox, true);
            match value {
                Value::Bool(true) => {
                    checkbox.set_inconsistent(false);
                    checkbox.set_active(true);
                }
                Value::Bool(false) => {
                    checkbox.set_inconsistent(false);
                    checkbox.set_active(false);
                }
                Value::Null => {
                    checkbox.set_inconsistent(true);
                    checkbox.set_active(false);
                }
                _ => {
                    checkbox.set_inconsistent(true);
                    checkbox.set_active(false);
                }
            }
            SUPPRESS_SLOT.set(&checkbox, false);
            clear_pending_classes(checkbox.upcast_ref());
            for cls in &pending_classes {
                checkbox.add_css_class(cls);
            }
            checkbox.set_opacity(if is_pending_delete { 0.5 } else { 1.0 });
            POSITION_SLOT.set(&checkbox, item.position());
            ROW_KEY_SLOT.set(&checkbox, pk_values.clone());
        } else if let Ok(label) = child.downcast::<gtk4::Label>() {
            label.set_text(&text);
            apply_cell_tooltip(label.upcast_ref(), &text, is_null);
            if is_null {
                label.add_css_class("dim-label");
            } else {
                label.remove_css_class("dim-label");
            }
            clear_pending_classes(label.upcast_ref());
            for cls in &pending_classes {
                label.add_css_class(cls);
            }
            set_label_strikethrough(&label, is_pending_delete);
            POSITION_SLOT.set(&label, item.position());
        }
    });

    factory.connect_unbind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(child) = item.child() else { return };
        if let Ok(label) = child.clone().downcast::<crate::ui::cell_editor::CellEditor>() {
            label.set_inline_editable(false);
            if label.is_editing() {
                label.stop_editing(false);
            }
            if let Some(popover) = POPOVER_SLOT.take(&label) {
                popover.popdown();
            }
            POSITION_SLOT.take(&label);
            ROW_KEY_SLOT.take(&label);
            SNAPSHOT_SLOT.take(&label);
        } else if let Ok(checkbox) = child.clone().downcast::<gtk4::CheckButton>() {
            checkbox.set_sensitive(false);
            POSITION_SLOT.take(&checkbox);
            ROW_KEY_SLOT.take(&checkbox);
        } else if let Ok(label) = child.downcast::<gtk4::Label>() {
            POSITION_SLOT.take(&label);
        }
    });

    let title = column_header_title(&info.name, tab_ctx.foreign_key_columns.contains(&info.name));
    let column = gtk4::ColumnViewColumn::builder()
        .title(&title)
        .factory(&factory)
        .resizable(true)
        .expand(true)
        .build();
    if sort_sender.is_some() {
        let dummy = gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal);
        column.set_sorter(Some(&dummy));
    }
    if let Some(id) = connection_id {
        if let Some(saved) = column_widths
            .as_ref()
            .and_then(|store| store.load(id, &table_for_persist, &info.name))
        {
            column.set_fixed_width(saved);
        } else if let Some(min) = default_min_width {
            column.set_fixed_width(min);
        }
        let column_for_save = column.clone();
        let column_name = info.name.clone();
        let column_widths = column_widths.clone();
        column.connect_fixed_width_notify(move |_| {
            let width = column_for_save.fixed_width();
            if width > 0
                && let Some(store) = &column_widths
                && let Err(error) = store.save(id, &table_for_persist, &column_name, width)
            {
                tracing::warn!(%error, "column width was not saved");
            }
        });
    } else if let Some(min) = default_min_width {
        column.set_fixed_width(min);
    }
    column
}

fn clear_pending_classes(widget: &gtk4::Widget) {
    for cls in PENDING_CSS_CLASSES {
        widget.remove_css_class(cls);
    }
}

fn apply_cell_tooltip(widget: &gtk4::Widget, text: &str, is_null: bool) {
    if is_null || text.chars().take(TOOLTIP_MIN_CHARS + 1).count() <= TOOLTIP_MIN_CHARS {
        widget.set_tooltip_text(None);
    } else {
        widget.set_tooltip_text(Some(text));
    }
}

fn set_label_strikethrough(label: &gtk4::Label, on: bool) {
    if on {
        let attrs = gtk4::pango::AttrList::new();
        attrs.insert(gtk4::pango::AttrInt::new_strikethrough(true));
        label.set_attributes(Some(&attrs));
    } else {
        label.set_attributes(None);
    }
}

#[cfg(test)]
mod tests {
    use super::super::display::{POSITION_SLOT, value_is_inline_editable};
    use super::*;
    use tablepro_core::{ColumnInfo, Value};

    fn col(data_type: &str, primary_key: bool) -> ColumnInfo {
        ColumnInfo {
            name: "x".into(),
            data_type: data_type.into(),
            nullable: true,
            primary_key,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
        }
    }

    #[test]
    fn editable_for_normal_column() {
        assert!(is_cell_editable(&col("text", false)));
        assert!(is_cell_editable(&col("integer", false)));
    }

    #[test]
    fn not_editable_for_primary_key() {
        assert!(!is_cell_editable(&col("integer", true)));
    }

    #[test]
    fn not_editable_for_generated_column() {
        let mut c = col("integer", false);
        c.is_generated = true;
        assert!(!is_cell_editable(&c));
    }

    #[test]
    fn not_editable_for_auto_increment_non_pk() {
        let mut c = col("integer", false);
        c.is_auto_increment = true;
        assert!(!is_cell_editable(&c));
    }

    #[test]
    fn header_title_marks_a_foreign_key_column() {
        assert_eq!(column_header_title("customer_id", true), "customer_id \u{1F517}");
    }

    #[test]
    fn header_title_leaves_a_plain_column_unmarked() {
        assert_eq!(column_header_title("name", false), "name");
    }

    #[test]
    fn not_editable_for_bytes() {
        assert!(!is_cell_editable(&col("bytea", false)));
        assert!(!is_cell_editable(&col("blob", false)));
        assert!(!is_cell_editable(&col("longblob", false)));
        assert!(!is_cell_editable(&col("BINARY", false)));
        assert!(!is_cell_editable(&col("varbinary", false)));
    }

    #[test]
    fn text_column_stays_column_editable_when_a_cell_holds_bytes() {
        assert!(is_cell_editable(&col("text", false)));
        assert!(!value_is_inline_editable(&Value::Bytes(vec![0xFF, 0xFE])));
    }

    fn collect_cell_editors(widget: &gtk4::Widget) -> Vec<crate::ui::cell_editor::CellEditor> {
        use gtk4::prelude::*;
        let mut found = Vec::new();
        if let Ok(editor) = widget.clone().downcast::<crate::ui::cell_editor::CellEditor>() {
            found.push(editor);
            return found;
        }
        let mut child = widget.first_child();
        while let Some(node) = child {
            found.extend(collect_cell_editors(&node));
            child = node.next_sibling();
        }
        found
    }

    fn wait_for_editors(view: &gtk4::Widget, n: usize) -> Vec<crate::ui::cell_editor::CellEditor> {
        let ctx = gtk4::glib::MainContext::default();
        for _ in 0..200 {
            let mut pumped = 0;
            while ctx.iteration(false) && pumped < 64 {
                pumped += 1;
            }
            let found = collect_cell_editors(view);
            if found.len() >= n {
                return found;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        collect_cell_editors(view)
    }

    fn editor_at(editors: &[crate::ui::cell_editor::CellEditor], position: u32) -> &crate::ui::cell_editor::CellEditor {
        editors
            .iter()
            .find(|editor| POSITION_SLOT.get(*editor) == Some(position))
            .unwrap_or_else(|| panic!("no cell editor bound at position {position}"))
    }

    fn capture_bind_visual(
        editors: &[crate::ui::cell_editor::CellEditor],
        artifact_dir: &std::path::Path,
    ) -> std::path::PathBuf {
        let log = artifact_dir.join("text-column-binary-bind.txt");
        let mut lines = String::from("position\ttext\tinline_editable\tediting\n");
        for position in 0..3 {
            let editor = editor_at(editors, position);
            lines.push_str(&format!(
                "{position}\t{}\t{}\t{}\n",
                editor.text(),
                editor.is_inline_editable(),
                editor.is_editing()
            ));
        }
        std::fs::write(&log, lines).expect("write bind visual log");
        let png = artifact_dir.join("text-column-binary-bind.png");
        let _ = std::process::Command::new("scrot").arg(&png).status();
        png
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn boolean_affinity_mismatch_cannot_emit_an_edit() {
        use futures::FutureExt;
        fn checkbox(widget: &gtk4::Widget) -> Option<gtk4::CheckButton> {
            if let Ok(button) = widget.clone().downcast::<gtk4::CheckButton>() {
                return Some(button);
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                if let Some(button) = checkbox(&current) {
                    return Some(button);
                }
                child = current.next_sibling();
            }
            None
        }
        gtk4::init().unwrap();
        let columns = vec![col("BOOLEAN", false)];
        let result = tablepro_core::QueryResult {
            columns: columns.clone(),
            rows: vec![vec![Value::Bool(false)]],
            truncated: false,
        };
        let (sender, receiver) = relm4::channel::<GridMsg>();
        let (view, selection) = crate::ui::grid::build_column_view(
            &result,
            &columns,
            "flags",
            Some(sender),
            None,
            None,
            None,
            None,
            TabGridContext::default(),
            None,
            std::sync::Arc::new(crate::services::database_service::DatabaseService::new()),
        );
        let window = gtk4::Window::builder().child(&view).build();
        window.present();
        let store = selection.model().unwrap().downcast::<gtk4::gio::ListStore>().unwrap();
        for (value, allowed) in [
            (Value::Bytes(vec![1]), false),
            (Value::Text("true".into()), false),
            (Value::Null, true),
            (Value::Bool(false), true),
            (Value::Bytes(vec![]), false),
        ] {
            store.splice(0, 1, &[crate::ui::row_object::RowObject::new(vec![value])]);
            let context = gtk4::glib::MainContext::default();
            for _ in 0..50 {
                for _ in 0..64 {
                    if !context.iteration(false) {
                        break;
                    }
                }
                if checkbox(view.upcast_ref()).is_some() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let button = checkbox(view.upcast_ref()).expect("bound checkbox");
            assert_eq!(button.is_sensitive(), allowed);
            button.set_active(!button.is_active());
            let event = receiver.recv().now_or_never().flatten();
            assert_eq!(
                matches!(event, Some(GridMsg::CellEdited { .. })),
                allowed,
                "blocked values must emit no pending edit"
            );
        }
        window.close();
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn bind_does_not_open_binary_bytes_in_a_text_column_for_editing() {
        use gtk4::prelude::*;
        gtk4::init().unwrap();
        let columns = vec![col("TEXT", false)];
        let result = tablepro_core::QueryResult {
            columns: columns.clone(),
            rows: vec![
                vec![Value::Text("hello".into())],
                vec![Value::Bytes(vec![0xFF, 0xFE, 0x00, 0x01])],
                vec![Value::Bytes(b"utf8 blob".to_vec())],
            ],
            truncated: false,
        };
        let (sender, _receiver) = relm4::channel::<GridMsg>();
        let (view, selection) = crate::ui::grid::build_column_view(
            &result,
            &columns,
            "notes",
            Some(sender),
            None,
            None,
            None,
            None,
            TabGridContext::default(),
            None,
            std::sync::Arc::new(crate::services::database_service::DatabaseService::new()),
        );
        let window = gtk4::Window::builder()
            .title("TEXT cells with binary values")
            .default_width(520)
            .default_height(220)
            .child(&view)
            .build();
        window.present();
        let editors = wait_for_editors(view.upcast_ref(), 3);
        assert_eq!(editors.len(), 3, "column factory should bind a cell editor per row");

        let text_cell = editor_at(&editors, 0);
        assert_eq!(text_cell.text().as_str(), "hello");
        assert!(text_cell.is_inline_editable());
        text_cell.start_editing();
        assert!(text_cell.is_editing());
        text_cell.stop_editing(false);
        assert!(!text_cell.is_editing());

        let binary_cell = editor_at(&editors, 1);
        assert_eq!(binary_cell.text().as_str(), "<4 bytes>");
        assert!(!binary_cell.is_inline_editable());
        binary_cell.start_editing();
        assert!(!binary_cell.is_editing());
        assert_eq!(binary_cell.text().as_str(), "<4 bytes>");

        let utf8_bytes_cell = editor_at(&editors, 2);
        assert_eq!(utf8_bytes_cell.text().as_str(), "utf8 blob");
        assert!(!utf8_bytes_cell.is_inline_editable());
        utf8_bytes_cell.start_editing();
        assert!(!utf8_bytes_cell.is_editing());

        let artifact_dir = std::env::var_os("TABLEPRO_GTK_ARTIFACT_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let _ = std::fs::create_dir_all(&artifact_dir);
        let png = capture_bind_visual(&editors, &artifact_dir);
        if std::env::var_os("TABLEPRO_GTK_REQUIRE_SCREENSHOT").is_some() {
            assert!(
                png.is_file(),
                "scrot should capture the bound TEXT/binary grid to {png:?}"
            );
        }

        let store = selection
            .model()
            .expect("selection model")
            .downcast::<gtk4::gio::ListStore>()
            .expect("row store");
        store.remove(0);
        store.insert(
            0,
            &crate::ui::row_object::RowObject::new(vec![Value::Bytes(vec![0xAA, 0xBB, 0xCC])]),
        );
        let rebound = wait_for_editors(view.upcast_ref(), 3);
        let recycled = editor_at(&rebound, 0);
        assert_eq!(recycled.text().as_str(), "<3 bytes>");
        assert!(!recycled.is_inline_editable());
        recycled.start_editing();
        assert!(!recycled.is_editing());

        store.remove(0);
        store.insert(
            0,
            &crate::ui::row_object::RowObject::new(vec![Value::Text("hello".into())]),
        );
        let restored = wait_for_editors(view.upcast_ref(), 3);
        let restored_cell = editor_at(&restored, 0);
        assert_eq!(restored_cell.text().as_str(), "hello");
        assert!(restored_cell.is_inline_editable());
        restored_cell.start_editing();
        assert!(restored_cell.is_editing());
        restored_cell.stop_editing(false);

        window.close();
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn entering_edit_on_a_truncated_long_text_cell_seeds_the_full_value() {
        use gtk4::prelude::*;
        gtk4::init().unwrap();
        let columns = vec![col("TEXT", false)];
        let long = "a".repeat(50_000);
        let result = tablepro_core::QueryResult {
            columns: columns.clone(),
            rows: vec![vec![Value::Text(long.clone())]],
            truncated: false,
        };
        let (sender, _receiver) = relm4::channel::<GridMsg>();
        let (view, _selection) = crate::ui::grid::build_column_view(
            &result,
            &columns,
            "notes",
            Some(sender),
            None,
            None,
            None,
            None,
            TabGridContext::default(),
            None,
            std::sync::Arc::new(crate::services::database_service::DatabaseService::new()),
        );
        let window = gtk4::Window::builder().child(&view).build();
        window.present();
        let editors = wait_for_editors(view.upcast_ref(), 1);
        let cell = editor_at(&editors, 0);
        assert!(cell.text().contains("more chars"), "display text should stay truncated");

        super::super::editing::enter_edit_mode(cell);
        assert!(cell.is_editing());
        assert_eq!(
            cell.entry().text().as_str(),
            long,
            "editing must seed the untruncated value"
        );
        cell.stop_editing(false);

        window.close();
    }
}
