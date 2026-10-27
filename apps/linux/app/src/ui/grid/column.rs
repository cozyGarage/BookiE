use gtk4::prelude::*;

use tablepro_core::{ColumnInfo, Value};

use super::context_menu::GridMenus;
use super::display::{
    COLUMN_SLOT, CellView, FULL_EDIT_KIND_SLOT, FULL_EDIT_TEXT_SLOT, IS_NULL_SLOT, POPOVER_SLOT, POSITION_SLOT,
    ROW_KEY_SLOT, SNAPSHOT_SLOT, SUPPRESS_SLOT, cell_view, value_to_full_edit_text,
};
use super::editing::{setup_bool_cell, setup_editable_cell, setup_readonly_cell};
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

pub(super) fn header_column(column: &ColumnInfo, schema: Option<&ColumnInfo>) -> ColumnInfo {
    let mut header = column.clone();
    let known = schema.filter(|known| known.name == column.name);
    header.primary_key |= known.is_some_and(|known| known.primary_key);
    if header.comment.is_none() {
        header.comment = known.and_then(|known| known.comment.clone());
    }
    header
}

fn column_header_title(info: &ColumnInfo, is_foreign_key: bool) -> String {
    let mut title = info.name.clone();
    if info.primary_key {
        title.push_str(" \u{1F511}");
    }
    if is_foreign_key {
        title.push_str(" \u{1F517}");
    }
    if !info.data_type.is_empty() {
        title.push('\n');
        title.push_str(&info.data_type);
    }
    title
}

fn header_details(info: &ColumnInfo, foreign_key: bool) -> gtk4::gio::Menu {
    let details = gtk4::gio::Menu::new();
    for text in [
        crate::tr!("Column: {name}").replace("{name}", &info.name),
        crate::tr!("Type: {type}").replace("{type}", &info.data_type),
        if info.nullable {
            crate::tr!("NULL allowed")
        } else {
            crate::tr!("NULL not allowed")
        },
    ] {
        details.append(Some(&text), None);
    }
    if info.primary_key {
        details.append(Some(&crate::tr!("Primary key")), None);
    }
    if foreign_key {
        details.append(Some(&crate::tr!("Foreign key")), None);
    }
    if let Some(comment) = info.comment.as_deref().filter(|comment| !comment.trim().is_empty()) {
        let text: String = comment.chars().take(1024).collect();
        details.append(Some(&text), None);
    }
    let menu = gtk4::gio::Menu::new();
    menu.append_section(Some(&crate::tr!("Column details")), &details);
    menu
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_column(
    info: &ColumnInfo,
    idx: usize,
    editable: bool,
    table: String,
    sender: Option<relm4::Sender<GridMsg>>,
    connection_id: Option<uuid::Uuid>,
    tab_ctx: TabGridContext,
    sort_enabled: bool,
    default_min_width: Option<i32>,
    grid_menus: Option<GridMenus>,
    column_widths: Option<crate::services::column_widths::ColumnWidthStore>,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();
    let edit_sender = if editable { sender.clone() } else { None };
    let readonly_sender = sender.clone();
    let table_for_persist = table;

    let column_data_type = info.data_type.clone();
    let choices_for_setup = tab_ctx.choices.clone();
    let column_name = info.name.clone();
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
                    grid_menus_for_setup.as_ref(),
                );
            } else {
                let editor_kind = classify_editor_kind(&column_data_type);
                setup_editable_cell(
                    item,
                    idx,
                    column_name.clone(),
                    super::full_value_editor::is_full_value_column(&column_data_type),
                    edit_sender,
                    editor_kind,
                    choices_for_setup.clone(),
                    grid_menus_for_setup.as_ref(),
                );
            }
        } else {
            setup_readonly_cell(
                item,
                idx,
                column_name.clone(),
                readonly_sender.clone(),
                grid_menus_for_setup.as_ref(),
            );
        }
    });

    let editable_for_bind = editable && sender.is_some();
    let column_info = info.clone();
    let tab_ctx_for_bind = tab_ctx.clone();
    factory.connect_bind(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(row) = item.item().and_downcast::<crate::ui::row_object::RowObject>() else {
            return;
        };
        let pk_values: Vec<Value> = tab_ctx_for_bind
            .pk_col_indices
            .iter()
            .map(|&i| row.cell_value(i))
            .collect();
        let (edited, updated_value) = if let Some(tab_id) = tab_ctx_for_bind.tab_id
            && row.draft_id().is_none()
        {
            crate::services::change_tracker::with_tab_ref(tab_id, |t| {
                crate::services::change_tracker::RowKey::from_pk_values(&pk_values)
                    .map(|key| {
                        let edited = t.cell_state(&key, idx) == crate::services::change_tracker::CellState::Modified;
                        let value = edited.then(|| {
                            let original = row.cell_value(idx);
                            t.current_cell_value(&key, idx, &original).clone()
                        });
                        (edited, value)
                    })
                    .unwrap_or((false, None))
            })
            .unwrap_or((false, None))
        } else {
            (false, None)
        };
        let preview = preview_for_bind(&row, idx, edited);
        let value = value_for_bind(&row, idx, preview.as_ref(), updated_value);
        let view = match (preview.as_ref(), value.as_ref()) {
            (Some(preview), _) => CellView::from_preview(preview),
            (None, Some(value)) => cell_view(value, &column_info),
            (None, None) => return,
        };
        let is_null = view.is_null;
        let inline_editable = editable_for_bind && view.inline_editable;
        let text = view.text_for_bind(editable_for_bind);

        let pending_classes: Vec<&'static str> = if let Some(_tab_id) = tab_ctx_for_bind.tab_id {
            if row.draft_id().is_some() {
                vec!["tp-row-pending-insert"]
            } else {
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
            IS_NULL_SLOT.set(&label, view.is_null);
            label.set_inline_editable(inline_editable);
            label.set_text(text.as_ref());
            apply_cell_tooltip(label.upcast_ref(), text.as_ref(), is_null);
            if is_null && !editable_for_bind {
                label.add_css_class("dim-label");
            } else {
                label.remove_css_class("dim-label");
            }
            if is_null && (editable_for_bind || view.auto_filled) {
                label.add_css_class("tp-null-sentinel");
            } else {
                label.remove_css_class("tp-null-sentinel");
            }
            clear_pending_classes(label.upcast_ref());
            for cls in &pending_classes {
                label.add_css_class(cls);
            }
            apply_pending_description(label.upcast_ref(), &pending_classes);
            label.set_strikethrough(is_pending_delete);
            POSITION_SLOT.set(&label, item.position());
            ROW_KEY_SLOT.set(&label, pk_values);
            if inline_editable && let Some(value) = &value {
                FULL_EDIT_TEXT_SLOT.set(&label, value_to_full_edit_text(value));
                if let Some(kind) = super::full_value_editor::edit_kind(&column_info.data_type, value) {
                    FULL_EDIT_KIND_SLOT.set(&label, kind);
                } else {
                    FULL_EDIT_KIND_SLOT.take(&label);
                }
            } else {
                FULL_EDIT_TEXT_SLOT.take(&label);
                FULL_EDIT_KIND_SLOT.take(&label);
            }
        } else if let Ok(checkbox) = child.clone().downcast::<gtk4::CheckButton>() {
            checkbox.set_sensitive(inline_editable);
            SUPPRESS_SLOT.set(&checkbox, true);
            if let Some(checked) = view.checked {
                checkbox.set_inconsistent(false);
                checkbox.set_active(checked);
            } else {
                checkbox.set_inconsistent(true);
                checkbox.set_active(false);
            }
            SUPPRESS_SLOT.set(&checkbox, false);
            clear_pending_classes(checkbox.upcast_ref());
            for cls in &pending_classes {
                checkbox.add_css_class(cls);
            }
            apply_pending_description(checkbox.upcast_ref(), &pending_classes);
            checkbox.set_opacity(if is_pending_delete { 0.5 } else { 1.0 });
            POSITION_SLOT.set(&checkbox, item.position());
            ROW_KEY_SLOT.set(&checkbox, pk_values);
        } else if let Ok(label) = child.downcast::<gtk4::Label>() {
            label.set_text(text.as_ref());
            apply_cell_tooltip(label.upcast_ref(), text.as_ref(), is_null);
            if is_null {
                label.add_css_class("dim-label");
            } else {
                label.remove_css_class("dim-label");
            }
            clear_pending_classes(label.upcast_ref());
            for cls in &pending_classes {
                label.add_css_class(cls);
            }
            apply_pending_description(label.upcast_ref(), &pending_classes);
            set_label_strikethrough(&label, is_pending_delete);
            POSITION_SLOT.set(&label, item.position());
            super::editing::sync_readonly_cell_focus(&label);
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
            clear_pending_classes(label.upcast_ref());
            label.reset_property(gtk4::AccessibleProperty::Description);
            label.set_strikethrough(false);
            POSITION_SLOT.take(&label);
            ROW_KEY_SLOT.take(&label);
            SNAPSHOT_SLOT.take(&label);
            IS_NULL_SLOT.take(&label);
        } else if let Ok(checkbox) = child.clone().downcast::<gtk4::CheckButton>() {
            checkbox.set_sensitive(false);
            clear_pending_classes(checkbox.upcast_ref());
            checkbox.reset_property(gtk4::AccessibleProperty::Description);
            checkbox.set_opacity(1.0);
            POSITION_SLOT.take(&checkbox);
            ROW_KEY_SLOT.take(&checkbox);
        } else if let Ok(label) = child.downcast::<gtk4::Label>() {
            clear_pending_classes(label.upcast_ref());
            label.reset_property(gtk4::AccessibleProperty::Description);
            set_label_strikethrough(&label, false);
            POSITION_SLOT.take(&label);
            if let Some(cell) = label
                .parent()
                .filter(|cell| cell.accessible_role() == gtk4::AccessibleRole::GridCell)
            {
                POSITION_SLOT.take(&cell);
                COLUMN_SLOT.take(&cell);
            }
        }
    });

    let title = column_header_title(info, tab_ctx.foreign_key_columns.contains(&info.name));
    let column = gtk4::ColumnViewColumn::builder()
        .title(&title)
        .factory(&factory)
        .resizable(true)
        .expand(true)
        .build();
    column.set_header_menu(Some(&header_details(
        info,
        tab_ctx.foreign_key_columns.contains(&info.name),
    )));
    if sort_enabled {
        let dummy = gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal);
        column.set_sorter(Some(&dummy));
    }
    if let Some(id) = connection_id {
        if let Some(saved) = column_widths
            .as_ref()
            .and_then(|store| store.load(id, &table_for_persist, &info.name))
        {
            column.set_expand(false);
            column.set_fixed_width(saved);
        } else if let Some(min) = default_min_width {
            column.set_fixed_width(min);
        }
        let column_name = info.name.clone();
        let column_widths = column_widths.clone();
        column.connect_fixed_width_notify(move |column| {
            let width = column.fixed_width();
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
    column.connect_fixed_width_notify(|column| column.set_expand(column.fixed_width() < 0));
    column
}

fn preview_for_bind(
    row: &crate::ui::row_object::RowObject,
    index: usize,
    edited: bool,
) -> Option<crate::ui::row_object::CellPreview> {
    (!edited).then(|| row.cell_preview(index)).flatten()
}

fn value_for_bind(
    row: &crate::ui::row_object::RowObject,
    index: usize,
    preview: Option<&crate::ui::row_object::CellPreview>,
    updated_value: Option<Value>,
) -> Option<Value> {
    preview
        .is_none()
        .then(|| updated_value.unwrap_or_else(|| row.cell_value(index)))
}

fn clear_pending_classes(widget: &gtk4::Widget) {
    for cls in PENDING_CSS_CLASSES {
        widget.remove_css_class(cls);
    }
}

fn pending_description(classes: &[&str]) -> Option<String> {
    if classes.contains(&"tp-row-pending-delete") {
        return Some(crate::tr!("Pending deletion").to_string());
    }
    if classes.contains(&"tp-row-pending-insert") {
        return Some(crate::tr!("New row").to_string());
    }
    classes
        .contains(&"tp-cell-modified")
        .then(|| crate::tr!("Modified").to_string())
}

fn apply_pending_description(widget: &gtk4::Widget, classes: &[&str]) {
    if let Some(description) = pending_description(classes) {
        widget.update_property(&[gtk4::accessible::Property::Description(&description)]);
    } else {
        widget.reset_property(gtk4::AccessibleProperty::Description);
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
#[path = "column_tests.rs"]
mod tests;

#[cfg(test)]
mod metadata_presentation_tests {
    use super::*;

    #[test]
    fn engine_type_identity_is_preserved_in_the_header() {
        let info = ColumnInfo {
            name: "value".into(),
            data_type: "public.MixedCaseType".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            is_generated: false,
            default_value: None,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        };
        assert_eq!(column_header_title(&info, false), "value\npublic.MixedCaseType");
    }
}
