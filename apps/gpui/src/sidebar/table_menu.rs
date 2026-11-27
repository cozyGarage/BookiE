use bookie_core::TableInfo;
use bookie_core::sql_dialect::qualified_table;
use bookie_gpui::shell::table_label;
use gpui::{ClipboardItem, Context, WeakEntity, Window};
use gpui_component::menu::{PopupMenu, PopupMenuItem};

use crate::GridView;
use crate::catalog_nav::TableOpenAction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableMenuItem {
    OpenData,
    Structure,
    CreateScript,
    SelectQuery,
    CopyName,
    Export,
    Empty,
    Drop,
}

impl TableMenuItem {
    pub(crate) fn label(self) -> &'static str {
        match self {
            TableMenuItem::OpenData => "Open data",
            TableMenuItem::Structure => "Structure",
            TableMenuItem::CreateScript => "Show CREATE statement",
            TableMenuItem::SelectQuery => "SELECT in the query editor",
            TableMenuItem::CopyName => "Copy name",
            TableMenuItem::Export => "Export table…",
            TableMenuItem::Empty => "Empty table…",
            TableMenuItem::Drop => "Drop table…",
        }
    }

    fn destructive(self) -> bool {
        matches!(self, TableMenuItem::Empty | TableMenuItem::Drop)
    }
}

pub(crate) fn table_menu_groups(connected: bool, writable: bool) -> Vec<Vec<TableMenuItem>> {
    let mut groups = vec![vec![TableMenuItem::OpenData]];
    if connected {
        groups[0].extend([
            TableMenuItem::Structure,
            TableMenuItem::CreateScript,
            TableMenuItem::SelectQuery,
        ]);
    }
    groups.push(vec![TableMenuItem::CopyName]);
    if connected {
        groups[1].push(TableMenuItem::Export);
    }
    if connected && writable {
        groups.push(vec![TableMenuItem::Empty, TableMenuItem::Drop]);
    }
    groups
}

fn menu_item(item: TableMenuItem, owner: WeakEntity<GridView>, table: TableInfo) -> PopupMenuItem {
    let entry = if item.destructive() {
        crate::menus::destructive_item(item.label(), None, None)
    } else {
        crate::menus::item(item.label(), None, None, false)
    };
    entry.on_click(move |_, window, cx| {
        let _ = owner.update(cx, |view, cx| {
            view.run_table_menu_item(item, &table, window, cx);
            cx.notify();
        });
    })
}

impl GridView {
    pub(crate) fn table_menu_items(&self) -> Vec<Vec<TableMenuItem>> {
        let writable = self.live.as_ref().is_some_and(|live| live.writable.is_some());
        table_menu_groups(self.live.is_some(), writable)
    }

    pub(crate) fn open_table_menu(&mut self, table: &TableInfo, window: &mut Window, cx: &mut Context<Self>) {
        let groups = self.table_menu_items();
        let owner = cx.entity().downgrade();
        let target = table.clone();
        let menu = PopupMenu::build(window, cx, move |mut menu, _, _| {
            for (index, group) in groups.iter().enumerate() {
                if index > 0 {
                    menu = menu.separator();
                }
                for item in group {
                    menu = menu.item(menu_item(*item, owner.clone(), target.clone()));
                }
            }
            menu
        });
        self.show_row_menu(menu, window, cx);
    }

    pub(crate) fn run_table_menu_item(
        &mut self,
        item: TableMenuItem,
        table: &TableInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.row_menu = None;
        let action = match item {
            TableMenuItem::OpenData => TableOpenAction::Data,
            TableMenuItem::Structure => TableOpenAction::Structure,
            TableMenuItem::CreateScript => TableOpenAction::CreateScript,
            TableMenuItem::Export => TableOpenAction::Export,
            TableMenuItem::Empty => TableOpenAction::Empty,
            TableMenuItem::Drop => TableOpenAction::Drop,
            TableMenuItem::CopyName => {
                cx.write_to_clipboard(ClipboardItem::new_string(table_label(table)));
                self.status = format!("Copied {}", table_label(table));
                return;
            }
            TableMenuItem::SelectQuery => {
                self.open_select_query(table, window, cx);
                return;
            }
        };
        self.open_table_for(table, action, window, cx);
    }

    fn open_select_query(&mut self, table: &TableInfo, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = &self.live else { return };
        let sql = format!(
            "SELECT * FROM {};",
            qualified_table(&live.driver_id, table.schema.as_deref(), &table.name)
        );
        self.tabs.open_query();
        self.sync_grid_owner();
        self.set_sql_text(&sql, window, cx);
        self.sql_input.update(cx, |input, cx| input.focus(window, cx));
    }
}
