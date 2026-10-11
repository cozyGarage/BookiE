use relm4::adw::prelude::*;
use relm4::{adw, gtk};

use crate::tr;

pub(crate) fn hidden_from_switches(schemas: &[String], active: &[bool]) -> Vec<String> {
    schemas
        .iter()
        .zip(active)
        .filter(|(_, on)| !**on)
        .map(|(schema, _)| schema.clone())
        .collect()
}

fn schema_rows(group: &adw::PreferencesGroup, schemas: &[String], hidden: &[String]) -> Vec<adw::SwitchRow> {
    schemas
        .iter()
        .map(|schema| {
            let row = adw::SwitchRow::builder()
                .title(schema.as_str())
                .active(!hidden.contains(schema))
                .build();
            group.add(&row);
            row
        })
        .collect()
}

fn keep_apply_usable(rows: &[adw::SwitchRow], apply_button: &gtk::Button) {
    let rows_for_state = rows.to_vec();
    let button = apply_button.clone();
    let refresh = std::rc::Rc::new(move || button.set_sensitive(rows_for_state.iter().any(|row| row.is_active())));
    for row in rows {
        let refresh = refresh.clone();
        row.connect_active_notify(move |_| refresh());
    }
    refresh();
}

fn dialog_around(group: &adw::PreferencesGroup, header: &adw::HeaderBar) -> adw::Dialog {
    let dialog = adw::Dialog::builder().title(tr!("Filter schemas")).build();
    dialog.set_content_width(440);
    dialog.set_content_height(520);
    let scrolled = gtk::ScrolledWindow::builder().vexpand(true).child(group).build();
    let body = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    body.append(&scrolled);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(header);
    toolbar.set_content(Some(&body));
    dialog.set_child(Some(&toolbar));
    dialog
}

pub fn present(parent: &gtk::Window, schemas: Vec<String>, hidden: &[String], apply: impl Fn(Vec<String>) + 'static) {
    if schemas.len() < 2 {
        let alert = adw::AlertDialog::new(
            Some(&tr!("Filter schemas")),
            Some(&tr!(
                "This connection has only one schema, so there is nothing to filter."
            )),
        );
        alert.add_response("ok", &tr!("OK"));
        alert.present(Some(parent));
        return;
    }
    let group = adw::PreferencesGroup::builder()
        .title(tr!("Show these schemas in the sidebar"))
        .description(tr!(
            "Hidden schemas stay on the server and in your open tabs; only the sidebar list changes."
        ))
        .build();
    let rows = schema_rows(&group, &schemas, hidden);
    let show_all = gtk::Button::with_label(&tr!("Show all"));
    let apply_button = gtk::Button::with_label(&tr!("Apply"));
    apply_button.add_css_class("suggested-action");
    let rows_for_all = rows.clone();
    show_all.connect_clicked(move |_| rows_for_all.iter().for_each(|row| row.set_active(true)));
    keep_apply_usable(&rows, &apply_button);
    let header = adw::HeaderBar::new();
    header.pack_start(&show_all);
    header.pack_end(&apply_button);
    let dialog = dialog_around(&group, &header);
    let dialog_for_apply = dialog.clone();
    apply_button.connect_clicked(move |_| {
        let active: Vec<bool> = rows.iter().map(|row| row.is_active()).collect();
        apply(hidden_from_switches(&schemas, &active));
        dialog_for_apply.close();
    });
    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switches_that_are_off_become_the_hidden_list_in_schema_order() {
        let schemas: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(hidden_from_switches(&schemas, &[true, false, false]), ["b", "c"]);
        assert!(hidden_from_switches(&schemas, &[true, true, true]).is_empty());
    }
}
