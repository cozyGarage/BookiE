use bookie_core::TableInfo;
use relm4::adw::prelude::*;
use relm4::{adw, gtk};

use crate::ui::spacing;

use bookie_app_services::export_tables::MAX_TABLES_PER_EXPORT;

pub(crate) fn present(
    parent: &impl IsA<gtk::Widget>,
    tables: Vec<TableInfo>,
    accept: impl Fn(Vec<TableInfo>, Option<usize>) + 'static,
) {
    let (dialog, toolbar) = crate::ui::dialog_shell::new(&bookie_app_services::tr!("Export tables as CSV"), 520, 600);
    let (content, selection, limit, message, cancel, next) = form(&tables);
    toolbar.set_content(Some(&content));
    let weak = dialog.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
    });
    let weak = dialog.downgrade();
    next.connect_clicked(move |_| {
        let selected = selected_tables(&tables, &selection);
        if selected.is_empty() {
            message.set_label(&bookie_app_services::tr!("Choose at least one table."));
            return;
        }
        if selected.len() > MAX_TABLES_PER_EXPORT {
            message.set_label(&bookie_app_services::tr!("Choose at most 256 tables per export."));
            return;
        }
        let Ok(max_rows) = super::export_dialog::row_limit::choice(&limit) else {
            message.set_label(&bookie_app_services::tr!("Enter a whole number of at least 1."));
            return;
        };
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
        accept(selected, max_rows);
    });
    dialog.present(Some(parent));
}

fn selected_tables(tables: &[TableInfo], selection: &gtk::MultiSelection) -> Vec<TableInfo> {
    tables
        .iter()
        .enumerate()
        .filter(|(index, _)| selection.is_selected(*index as u32))
        .take(MAX_TABLES_PER_EXPORT + 1)
        .map(|(_, table)| table.clone())
        .collect()
}

fn form(
    tables: &[TableInfo],
) -> (
    gtk::Box,
    gtk::MultiSelection,
    adw::EntryRow,
    gtk::Label,
    gtk::Button,
    gtk::Button,
) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, spacing::MD);
    content.set_margin_top(spacing::LG);
    content.set_margin_bottom(spacing::LG);
    content.set_margin_start(spacing::LG);
    content.set_margin_end(spacing::LG);
    let title = gtk::Label::new(Some(&bookie_app_services::tr!(
        "Choose tables to export. Hold Ctrl to select more than one."
    )));
    title.set_xalign(0.0);
    title.set_wrap(true);
    content.append(&title);
    let detail = gtk::Label::new(Some(&bookie_app_services::tr!(
        "Each table is saved as a separate CSV file. Existing files are not replaced."
    )));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    content.append(&detail);
    let (selection, list) = table_list(tables);
    let scroll = gtk::ScrolledWindow::builder().vexpand(true).child(&list).build();
    content.append(&scroll);
    let limit = super::export_dialog::row_limit::build();
    let options = gtk::ListBox::new();
    options.set_selection_mode(gtk::SelectionMode::None);
    options.add_css_class("boxed-list");
    options.append(&limit);
    content.append(&options);
    let message = gtk::Label::new(None);
    message.set_wrap(true);
    message.set_xalign(0.0);
    content.append(&message);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, spacing::SM);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label(&bookie_app_services::tr!("Cancel"));
    let next = gtk::Button::with_label(&bookie_app_services::tr!("Choose destination"));
    next.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&next);
    content.append(&buttons);
    (content, selection, limit, message, cancel, next)
}

fn table_list(tables: &[TableInfo]) -> (gtk::MultiSelection, gtk::ListView) {
    let names = gtk::StringList::new(&[]);
    for table in tables {
        let label = match &table.schema {
            Some(schema) => format!("{schema}.{}", table.name),
            None => table.name.clone(),
        };
        names.append(&label);
    }
    let selection = gtk::MultiSelection::new(Some(names));
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        if let Some(item) = item.downcast_ref::<gtk::ListItem>() {
            let label = gtk::Label::new(None);
            label.set_xalign(0.0);
            label.set_margin_top(spacing::SM);
            label.set_margin_bottom(spacing::SM);
            label.set_margin_start(spacing::SM);
            label.set_margin_end(spacing::SM);
            item.set_child(Some(&label));
        }
    });
    factory.connect_bind(|_, item| {
        if let Some(item) = item.downcast_ref::<gtk::ListItem>()
            && let Some(value) = item.item().and_downcast::<gtk::StringObject>()
            && let Some(label) = item.child().and_downcast::<gtk::Label>()
        {
            label.set_label(&value.string());
        }
    });
    let list = gtk::ListView::new(Some(selection.clone()), Some(factory));
    list.update_property(&[gtk::accessible::Property::Label(&bookie_app_services::tr!(
        "Tables to export"
    ))]);
    (selection, list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn find<T: IsA<gtk::Widget> + Clone + 'static>(widget: &gtk::Widget) -> Option<T> {
        if let Some(found) = widget.downcast_ref::<T>() {
            return Some(found.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            if let Some(found) = find::<T>(&current) {
                return Some(found);
            }
        }
        None
    }

    fn destination_button(widget: &gtk::Widget) -> Option<gtk::Button> {
        if let Some(button) = widget.downcast_ref::<gtk::Button>()
            && button.label().as_deref() == Some("Choose destination")
        {
            return Some(button.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            if let Some(button) = destination_button(&current) {
                return Some(button);
            }
        }
        None
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn export_setup_refuses_invalid_inputs_and_returns_selected_tables() {
        adw::init().unwrap();
        let application = adw::Application::builder()
            .application_id("com.bookie.linux.multi-export-test")
            .build();
        application.register(None::<&gtk::gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&application);
        window.present();
        let tables = vec![
            TableInfo {
                schema: Some("public".into()),
                name: "a".into(),
            },
            TableInfo {
                schema: Some("public".into()),
                name: "b".into(),
            },
        ];
        let received = Rc::new(RefCell::new(None));
        let output = received.clone();
        present(&window, tables.clone(), move |selected, limit| {
            *output.borrow_mut() = Some((selected, limit))
        });
        let dialog = window.visible_dialog().unwrap();
        let list = find::<gtk::ListView>(dialog.upcast_ref()).unwrap();
        let selection = list.model().unwrap().downcast::<gtk::MultiSelection>().unwrap();
        let limit = find::<adw::EntryRow>(dialog.upcast_ref()).unwrap();
        let button = destination_button(dialog.upcast_ref()).unwrap();
        button.emit_clicked();
        assert!(received.borrow().is_none());
        selection.select_item(0, false);
        selection.select_item(1, false);
        limit.set_text("0");
        button.emit_clicked();
        assert!(received.borrow().is_none());
        limit.set_text("5");
        button.emit_clicked();
        assert_eq!(*received.borrow(), Some((tables, Some(5))));
        window.close();
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn selects_multiple_qualified_tables_and_validates_optional_limit() {
        gtk::init().unwrap();
        adw::init().unwrap();
        let tables: Vec<_> = [("first", "orders"), ("second", "orders"), ("first", "users")]
            .into_iter()
            .map(|(schema, name)| TableInfo {
                schema: Some(schema.into()),
                name: name.into(),
            })
            .collect();
        let (_, selection, limit, _, _, _) = form(&tables);
        assert!(selected_tables(&tables, &selection).is_empty());
        selection.select_item(0, false);
        selection.select_item(1, false);
        assert_eq!(selected_tables(&tables, &selection), tables[..2]);
        assert_eq!(super::super::export_dialog::row_limit::choice(&limit), Ok(None));
        limit.set_text("25");
        assert_eq!(super::super::export_dialog::row_limit::choice(&limit), Ok(Some(25)));
        limit.set_text("0");
        assert!(super::super::export_dialog::row_limit::choice(&limit).is_err());
    }
}
