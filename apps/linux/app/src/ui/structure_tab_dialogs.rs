use std::cell::RefCell;
use std::rc::Rc;

use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw, gtk};

use bookie_core::sql_ddl::DraftColumn;
use bookie_core::{ForeignKeyInfo, IndexInfo};

use super::structure_tab::{StructureTab, StructureTabInput, StructureTabOutput};

type ColumnChecks = Rc<RefCell<Vec<(String, gtk::CheckButton)>>>;

fn build_form_dialog(title: &str, submit_label: &str) -> (adw::Dialog, gtk::Box, gtk::Button) {
    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(420)
        .content_height(560)
        .build();

    let header = adw::HeaderBar::builder()
        .show_start_title_buttons(false)
        .show_end_title_buttons(false)
        .build();
    let cancel_btn = gtk::Button::with_label(&bookie_app_services::tr!("Cancel"));
    let submit_btn = gtk::Button::with_label(submit_label);
    submit_btn.add_css_class("suggested-action");
    header.pack_start(&cancel_btn);
    header.pack_end(&submit_btn);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    let scroller = gtk::ScrolledWindow::builder()
        .child(&content)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .hexpand(true)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&scroller));
    dialog.set_child(Some(&toolbar_view));
    // Keep Enter in an AdwEntryRow on the default Add action.
    dialog.set_default_widget(Some(&submit_btn));

    let dialog_for_cancel = dialog.downgrade();
    cancel_btn.connect_clicked(move |_| {
        if let Some(dialog) = dialog_for_cancel.upgrade() {
            dialog.close();
        }
    });

    crate::ui::leak_probe::track("structure_form", &dialog);
    (dialog, content, submit_btn)
}

fn section_label(title: &str) -> gtk::Label {
    let label = gtk::Label::builder().label(title).xalign(0.0).build();
    label.add_css_class("heading");
    label
}

fn build_column_checklist(columns: &[DraftColumn]) -> (gtk::ListBox, ColumnChecks) {
    let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).build();
    list.add_css_class("boxed-list");
    let checks: ColumnChecks = Rc::new(RefCell::new(Vec::new()));
    for col in columns {
        let row = adw::ActionRow::builder().use_markup(false).title(&col.name).build();
        let check = gtk::CheckButton::new();
        check.set_valign(gtk::Align::Center);
        row.add_suffix(&check);
        row.set_activatable_widget(Some(&check));
        list.append(&row);
        checks.borrow_mut().push((col.name.clone(), check));
    }
    (list, checks)
}

fn index_detail_group() -> (adw::PreferencesGroup, adw::EntryRow, adw::SwitchRow) {
    // AdwEntryRow floats its title when text is entered, so do not add a second name label.
    let detail_group = adw::PreferencesGroup::builder().build();
    let name_row = adw::EntryRow::builder().title(bookie_app_services::tr!("Name")).build();
    detail_group.add(&name_row);
    let unique_row = adw::SwitchRow::builder()
        .title(bookie_app_services::tr!("Unique"))
        .subtitle(bookie_app_services::tr!(
            "Reject inserts that duplicate the indexed columns"
        ))
        .build();
    detail_group.add(&unique_row);
    (detail_group, name_row, unique_row)
}

pub(super) fn present_index_dialog(
    parent: &gtk::Widget,
    columns: &[DraftColumn],
    sender: ComponentSender<StructureTab>,
) {
    let (dialog, body, submit_btn) =
        build_form_dialog(&bookie_app_services::tr!("Add Index"), &bookie_app_services::tr!("Add"));

    let (detail_group, name_row, unique_row) = index_detail_group();
    body.append(&detail_group);

    body.append(&section_label(&bookie_app_services::tr!("Columns")));
    let (columns_list, column_checks) = build_column_checklist(columns);
    body.append(&columns_list);

    let column_checks_for_resp = column_checks.clone();
    let sender_for_resp = sender.clone();
    let dialog_for_submit = dialog.downgrade();
    submit_btn.connect_clicked(move |_| {
        let Some(dialog_for_submit) = dialog_for_submit.upgrade() else {
            return;
        };
        let name = name_row.text().to_string();
        if name.trim().is_empty() {
            let _ = sender_for_resp.output(StructureTabOutput::ShowToast(bookie_app_services::tr!(
                "Index name is required."
            )));
            return;
        }
        let cols: Vec<String> = column_checks_for_resp
            .borrow()
            .iter()
            .filter_map(|(n, c)| if c.is_active() { Some(n.clone()) } else { None })
            .collect();
        if cols.is_empty() {
            let _ = sender_for_resp.output(StructureTabOutput::ShowToast(bookie_app_services::tr!(
                "Select at least one column."
            )));
            return;
        }
        sender_for_resp.input(StructureTabInput::AddIndex(IndexInfo {
            constraint: None,
            name,
            columns: cols,
            unique: unique_row.is_active(),
            primary: false,
            predicate: None,
            has_expression_key: false,
            using_clause: None,
        }));
        dialog_for_submit.close();
    });

    dialog.present(Some(parent));
}

pub(super) fn present_fk_dialog(
    parent: &gtk::Widget,
    columns: &[DraftColumn],
    driver_id: &str,
    sender: ComponentSender<StructureTab>,
) {
    let fk_actions = bookie_core::sql_ddl::supported_fk_actions(driver_id);
    let (dialog, body, submit_btn) = build_form_dialog(
        &bookie_app_services::tr!("Add Foreign Key"),
        &bookie_app_services::tr!("Add"),
    );

    let name_group = adw::PreferencesGroup::builder().build();
    let name_row = adw::EntryRow::builder().title(bookie_app_services::tr!("Name")).build();
    name_group.add(&name_row);
    body.append(&name_group);

    body.append(&section_label(&bookie_app_services::tr!("Source columns")));
    let (columns_list, column_checks) = build_column_checklist(columns);
    body.append(&columns_list);

    // Reference columns stay free-text; resolving them requires an asynchronous catalog read.
    let ref_group = adw::PreferencesGroup::builder()
        .title(bookie_app_services::tr!("References"))
        .build();
    let ref_table_row = adw::EntryRow::builder()
        .title(bookie_app_services::tr!("Table"))
        .build();
    ref_group.add(&ref_table_row);
    let ref_cols_row = adw::EntryRow::builder()
        .title(bookie_app_services::tr!("Columns"))
        .build();
    ref_group.add(&ref_cols_row);
    let on_delete_row = adw::ComboRow::builder()
        .title(bookie_app_services::tr!("On delete"))
        .model(&gtk::StringList::new(fk_actions))
        .build();
    ref_group.add(&on_delete_row);
    let on_update_row = adw::ComboRow::builder()
        .title(bookie_app_services::tr!("On update"))
        .model(&gtk::StringList::new(fk_actions))
        .build();
    ref_group.add(&on_update_row);
    body.append(&ref_group);

    let column_checks_for_resp = column_checks.clone();
    let sender_for_resp = sender.clone();
    let dialog_for_submit = dialog.downgrade();
    submit_btn.connect_clicked(move |_| {
        let Some(dialog_for_submit) = dialog_for_submit.upgrade() else {
            return;
        };
        let name = name_row.text().to_string();
        let ref_table = ref_table_row.text().to_string();
        if name.trim().is_empty() {
            let _ = sender_for_resp.output(StructureTabOutput::ShowToast(bookie_app_services::tr!(
                "Foreign key name is required."
            )));
            return;
        }
        if ref_table.trim().is_empty() {
            let _ = sender_for_resp.output(StructureTabOutput::ShowToast(bookie_app_services::tr!(
                "Reference table is required."
            )));
            return;
        }
        let cols: Vec<String> = column_checks_for_resp
            .borrow()
            .iter()
            .filter_map(|(n, c)| if c.is_active() { Some(n.clone()) } else { None })
            .collect();
        if cols.is_empty() {
            let _ = sender_for_resp.output(StructureTabOutput::ShowToast(bookie_app_services::tr!(
                "Select at least one source column."
            )));
            return;
        }
        let ref_cols: Vec<String> = ref_cols_row
            .text()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if ref_cols.is_empty() {
            let _ = sender_for_resp.output(StructureTabOutput::ShowToast(bookie_app_services::tr!(
                "Reference columns are required."
            )));
            return;
        }
        if ref_cols.len() != cols.len() {
            let _ = sender_for_resp.output(StructureTabOutput::ShowToast(bookie_app_services::tr!(
                "Source and reference column counts must match."
            )));
            return;
        }
        let (ref_schema, ref_table_only) = match ref_table.split_once('.') {
            Some((s, t)) => (Some(s.trim().to_string()), t.trim().to_string()),
            None => (None, ref_table),
        };
        // Keep an explicit NO ACTION distinct from a driver-unknown action.
        let action_at = |idx: u32| -> Option<String> { fk_actions.get(idx as usize).map(|s| (*s).to_string()) };
        sender_for_resp.input(StructureTabInput::AddForeignKey(ForeignKeyInfo {
            name,
            columns: cols,
            ref_schema,
            ref_table: ref_table_only,
            ref_columns: ref_cols,
            on_delete: action_at(on_delete_row.selected()),
            on_update: action_at(on_update_row.selected()),
        }));
        dialog_for_submit.close();
    });

    dialog.present(Some(parent));
}

#[cfg(test)]
mod lifetime_tests;
