use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw, gtk};

use bookie_core::IndexInfo;

use super::{StructureTab, StructureTabInput};

fn index_badge(label: &str, accent_class: &str) -> gtk::Label {
    let badge = gtk::Label::builder().label(label).valign(gtk::Align::Center).build();
    badge.add_css_class("caption");
    badge.add_css_class(accent_class);
    badge
}

pub(super) fn build_index_row(index: usize, idx: &IndexInfo, sender: ComponentSender<StructureTab>) -> adw::ActionRow {
    // Empty columns array means the driver returned a malformed index
    // (corrupt catalog or driver bug). Render a dim "—" subtitle so
    // the user sees something rather than an empty cell.
    let subtitle = index_subtitle(idx);
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(&idx.name))
        .subtitle(glib::markup_escape_text(&subtitle))
        .build();

    if idx.unique {
        row.add_suffix(&index_badge(&bookie_app_services::tr!("UNIQUE"), "dim-label"));
    }
    if idx.primary {
        row.add_suffix(&index_badge(&bookie_app_services::tr!("PRIMARY"), "accent"));
    }

    let remove_button = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text(bookie_app_services::tr!("Remove index"))
        .valign(gtk::Align::Center)
        .build();
    remove_button.add_css_class("flat");
    // Primary index isn't user-droppable — it's owned by the PK
    // column constraint and removing it breaks the table.
    if idx.primary {
        remove_button.set_sensitive(false);
        remove_button.set_tooltip_text(Some(&bookie_app_services::tr!(
            "Primary-key index can't be dropped here; clear the PK on the column."
        )));
    }
    let sender_for_remove = sender.clone();
    remove_button.connect_clicked(move |_| sender_for_remove.input(StructureTabInput::RemoveIndex(index)));
    row.add_suffix(&remove_button);

    row
}

fn index_subtitle(idx: &IndexInfo) -> String {
    let columns = if idx.columns.is_empty() {
        "—".to_string()
    } else {
        idx.columns.join(", ")
    };
    match &idx.predicate {
        Some(predicate) => format!("{columns} WHERE {predicate}"),
        None => columns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partial_index_subtitle_names_its_predicate() {
        let index = IndexInfo {
            constraint: None,
            name: "active_email".into(),
            columns: vec!["lower(email)".into()],
            unique: true,
            primary: false,
            predicate: Some("deleted_at IS NULL".into()),
            has_expression_key: false,
        };
        assert_eq!(index_subtitle(&index), "lower(email) WHERE deleted_at IS NULL");
    }
}
