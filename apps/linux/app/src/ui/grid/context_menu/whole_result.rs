use gtk4::prelude::*;
use gtk4::{self as gtk, gio};

use super::super::GridMsg;
use super::super::cell_context::CellContextStore;
use super::{position, source_value_at};
use crate::ui::editor::StatementIdentity;

pub(super) fn extend_menu(menu: &gio::Menu) {
    let section = gio::Menu::new();
    section.append(
        Some(&bookie_app_services::tr!("Show All Rows Equal to This Value")),
        Some("cell.whole-result-equals"),
    );
    section.append(
        Some(&bookie_app_services::tr!("Show All Rows Not Equal to This Value")),
        Some("cell.whole-result-not-equals"),
    );
    let before_actions = menu.n_items().saturating_sub(1);
    menu.insert_section(before_actions, None, &section);
}

pub(super) fn actions(
    context: CellContextStore,
    sender: relm4::Sender<GridMsg>,
    column_view: &gtk::ColumnView,
    identity: StatementIdentity,
) -> [gio::ActionEntry<gio::SimpleActionGroup>; 2] {
    [
        entry("whole-result-equals", false, &context, &sender, column_view, identity),
        entry(
            "whole-result-not-equals",
            true,
            &context,
            &sender,
            column_view,
            identity,
        ),
    ]
}

fn entry(
    name: &str,
    exclude: bool,
    context: &CellContextStore,
    sender: &relm4::Sender<GridMsg>,
    column_view: &gtk::ColumnView,
    identity: StatementIdentity,
) -> gio::ActionEntry<gio::SimpleActionGroup> {
    let context = context.clone();
    let sender = sender.clone();
    let view = column_view.downgrade();
    gio::ActionEntry::builder(name)
        .activate(move |_, _, _| {
            let context = context.snapshot();
            let Some(slot) = context.as_ref() else { return };
            let Some(view) = view.upgrade() else { return };
            let Some(value) = source_value_at(&view, position(slot), slot.col_index) else {
                return;
            };
            sender
                .send(GridMsg::FilterWholeResult {
                    identity,
                    column: slot.col_index,
                    value,
                    exclude,
                })
                .ok();
        })
        .build()
}
