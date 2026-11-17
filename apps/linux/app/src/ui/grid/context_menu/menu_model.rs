//! Popovers and gio menu models for the grid context menu.

use super::*;

pub(super) fn build_menu_popovers(column_view: &gtk::ColumnView, models: MenuModels) -> MenuPopovers {
    let editable = gtk::PopoverMenu::from_model_full(&models.editable, gtk::PopoverMenuFlags::NESTED);
    let readonly = gtk::PopoverMenu::from_model_full(&models.readonly, gtk::PopoverMenuFlags::NESTED);
    let empty = crate::ui::context_menu::from_model(Some(&models.empty));
    crate::ui::context_menu::configure(&editable);
    crate::ui::context_menu::configure(&readonly);
    editable.set_parent(column_view);
    readonly.set_parent(column_view);
    empty.set_parent(column_view);
    let editable_for_destroy = editable.clone();
    let readonly_for_destroy = readonly.clone();
    let empty_for_destroy = empty.clone();
    column_view.connect_destroy(move |_| {
        editable_for_destroy.unparent();
        readonly_for_destroy.unparent();
        empty_for_destroy.unparent();
    });
    MenuPopovers {
        editable,
        readonly,
        empty,
    }
}

pub(super) fn empty_grid_menu(row_mutations_allowed: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    if row_mutations_allowed {
        menu.append(Some(&bookie_app_services::tr!("Insert row")), Some("cell.insert-row"));
    }
    menu.append(
        Some(&bookie_app_services::tr!("Jump to Column…")),
        Some("grid.jump-column"),
    );
    menu
}

pub(super) fn set_cells_item() -> gio::MenuItem {
    let item = gio::MenuItem::new(
        Some(&bookie_app_services::tr!("Set Cells to Value…")),
        Some("cell.set-cells"),
    );
    item.set_attribute_value("hidden-when", Some(&"action-disabled".to_variant()));
    item
}

pub(super) fn export_loaded_item() -> gio::MenuItem {
    let item = gio::MenuItem::new(
        Some(&bookie_app_services::tr!("Export All Loaded Rows…")),
        Some("cell.export-loaded"),
    );
    item.set_attribute_value("hidden-when", Some(&"action-disabled".to_variant()));
    item
}

pub(super) fn build_copy_as_menu(insert_copy: bool) -> gio::Menu {
    let copy_as = gio::Menu::new();
    copy_as.append(Some(&bookie_app_services::tr!("Rows")), Some("cell.copy-rows"));
    copy_as.append(
        Some(&bookie_app_services::tr!("With Headers")),
        Some("cell.copy-rows-headers"),
    );
    copy_as.append(
        Some(&bookie_app_services::tr!("Text Table")),
        Some("cell.copy-text-table"),
    );
    copy_as.append(Some(&bookie_app_services::tr!("JSON")), Some("cell.copy-json"));
    copy_as.append(
        Some(&bookie_app_services::tr!("JSON Lines")),
        Some("cell.copy-json-lines"),
    );
    copy_as.append(Some(&bookie_app_services::tr!("CSV")), Some("cell.copy-csv"));
    copy_as.append(
        Some(&bookie_app_services::tr!("CSV with Headers")),
        Some("cell.copy-csv-headers"),
    );
    copy_as.append(Some(&bookie_app_services::tr!("Markdown")), Some("cell.copy-markdown"));
    copy_as.append(
        Some(&bookie_app_services::tr!("IN Clause")),
        Some("cell.copy-in-clause"),
    );
    copy_as.append(
        Some(&bookie_app_services::tr!("Values List")),
        Some("cell.copy-values-list"),
    );
    if insert_copy {
        copy_as.append(
            Some(&bookie_app_services::tr!("INSERT Statement")),
            Some("cell.copy-row-insert"),
        );
        copy_as.append(
            Some(&bookie_app_services::tr!("UPDATE Statement")),
            Some("cell.copy-row-update"),
        );
        copy_as.append(
            Some(&bookie_app_services::tr!("DELETE Statement")),
            Some("cell.copy-row-delete"),
        );
        copy_as.append(
            Some(&bookie_app_services::tr!("WHERE Clause")),
            Some("cell.copy-row-where"),
        );
    }
    copy_as
}

pub(super) fn edit_section() -> gio::Menu {
    let section = gio::Menu::new();
    let item = gio::MenuItem::new(Some(&bookie_app_services::tr!("Edit cell")), Some("cell.edit"));
    item.set_attribute_value("hidden-when", Some(&"action-disabled".to_variant()));
    section.append_item(&item);
    let full_value = gio::MenuItem::new(
        Some(&bookie_app_services::tr!("Edit Full Value…")),
        Some("cell.edit-full-value"),
    );
    full_value.set_attribute_value("hidden-when", Some(&"action-disabled".to_variant()));
    section.append_item(&full_value);
    section
}

pub(super) fn display_section(filterable: bool, local_result: bool) -> gio::Menu {
    let display = gio::Menu::new();
    if filterable || local_result {
        display.append(
            Some(&bookie_app_services::tr!("Filter by This Value")),
            Some("cell.filter-by-value"),
        );
        display.append(
            Some(&bookie_app_services::tr!("Exclude This Value")),
            Some("cell.exclude-value"),
        );
    }
    if filterable {
        display.append(
            Some(&bookie_app_services::tr!("Go to Referenced Row")),
            Some("cell.go-to-reference"),
        );
        display.append(
            Some(&bookie_app_services::tr!("Show Referencing Rows")),
            Some("cell.show-referencing-rows"),
        );
    }
    display.append(Some(&bookie_app_services::tr!("View Value…")), Some("cell.view-value"));
    display.append(
        Some(&bookie_app_services::tr!("Show Row as JSON")),
        Some("cell.show-row-json"),
    );
    display
}

pub(super) fn build_menu(
    editable: bool,
    row_operations: bool,
    insert_copy: bool,
    filterable: bool,
    local_result: bool,
) -> gio::Menu {
    let menu = gio::Menu::new();
    if editable {
        menu.append_section(None, &edit_section());
    }
    let copy = gio::Menu::new();
    copy.append(Some(&bookie_app_services::tr!("Copy value")), Some("cell.copy-value"));
    copy.append_submenu(
        Some(&bookie_app_services::tr!("Copy as")),
        &build_copy_as_menu(insert_copy),
    );
    copy.append(
        Some(&bookie_app_services::tr!("Copy column name")),
        Some("cell.copy-column-name"),
    );
    menu.append_section(None, &copy);
    menu.append_section(None, &display_section(filterable, local_result));
    let actions = gio::Menu::new();
    let export_label = if local_result {
        bookie_app_services::tr!("Export Visible Rows…")
    } else {
        bookie_app_services::tr!("Export Results…")
    };
    actions.append(Some(&export_label), Some("cell.export"));
    if local_result {
        actions.append_item(&export_loaded_item());
    }
    actions.append(Some(&bookie_app_services::tr!("Columns…")), Some("grid.columns"));
    actions.append(
        Some(&bookie_app_services::tr!("Jump to Column…")),
        Some("grid.jump-column"),
    );
    actions.append(
        Some(&bookie_app_services::tr!("Find in Loaded Rows…")),
        Some("grid.find-row"),
    );
    actions.append(
        Some(&bookie_app_services::tr!("Highlight Rules…")),
        Some("grid.highlight-rules"),
    );
    if row_operations {
        actions.append(Some(&bookie_app_services::tr!("Insert row")), Some("cell.insert-row"));
        actions.append(
            Some(&bookie_app_services::tr!("Duplicate row")),
            Some("cell.duplicate-row"),
        );
        actions.append(Some(&bookie_app_services::tr!("Set to NULL")), Some("cell.set-null"));
        actions.append_item(&set_cells_item());
        actions.append(
            Some(&bookie_app_services::tr!("Set to Default")),
            Some("cell.set-default"),
        );
        actions.append(
            Some(&bookie_app_services::tr!("Set to Expression…")),
            Some("cell.set-expression"),
        );
        actions.append(Some(&bookie_app_services::tr!("Delete row")), Some("cell.delete-row"));
    }
    menu.append_section(None, &actions);
    menu
}
