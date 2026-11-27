use bookie_gpui::theme::{Theme, size};
use gpui::{
    App, Context, DismissEvent, Entity, Pixels, Point, SharedString, Subscription, WeakEntity, Window, div, prelude::*,
    px, rgb,
};
use gpui_component::Icon;
use gpui_component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::TestSupportExt as _;

use crate::GridView;
use crate::ui_theme::ThemeExt as _;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub id: &'static str,
    pub label: String,
    pub enabled: bool,
    pub shortcut: Option<String>,
    pub checked: bool,
}

type Group = Vec<Entry>;

#[cfg(test)]
pub(crate) const MAX_GROUP_ITEMS: usize = 7;

pub(crate) const OVERFLOW_GROUPS: &[&[&str]] = &[
    &["query", "run", "explain", "data-dictionary"],
    &["sidebar", "results", "detail"],
    &[
        "new-connection",
        "home",
        "history",
        "audit",
        "settings",
        "font",
        "theme",
    ],
    &["status", "server-status"],
];

pub(crate) const IMPORT_EXPORT_GROUPS: &[&[&str]] = &[&["import-connections", "export-connections"]];

pub(crate) const FILE_GROUPS: &[&[&str]] = &[
    &["open-sql-file", "save-sql-file"],
    &["export-file", "export-csv", "import-csv"],
];

pub(crate) const EXPORT_GROUPS: &[&[&str]] = &[&["export-file", "export-csv"]];

pub(crate) const ROW_GROUPS: &[&[&str]] = &[
    &[
        "copy",
        "copy-csv",
        "copy-json",
        "copy-markdown",
        "copy-insert",
        "copy-in-list",
    ],
    &["show-row-vertically", "selection-stats", "count-table-rows"],
    &["edit", "set-cells", "clear-selection"],
    &["delete-rows"],
];

pub(crate) const PAGE_GROUPS: &[&[&str]] = &[&["first", "last"]];

pub(crate) const COLUMN_GROUPS: &[&[&str]] = &[&["jump-column", "fit-visible-columns", "reset-grid-widths"]];

pub(crate) const GRID_MORE_GROUPS: &[&[&str]] = &[
    &[
        "jump-column",
        "fit-visible-columns",
        "reset-grid-widths",
        "show-hidden-columns",
    ],
    &["refresh-table", "insert-row", "compare"],
    &["export-file", "export-csv"],
];

pub(crate) const DESTRUCTIVE: &[&str] = &[
    "delete-rows",
    "empty-table",
    "drop-table",
    "drop-column",
    "drop-index",
    "drop-foreign-key",
    "delete-favorite",
    "delete-saved-filter",
    "discard-view-edits",
];

#[cfg(test)]
pub(crate) const MENUS: &[&[&[&str]]] = &[
    OVERFLOW_GROUPS,
    IMPORT_EXPORT_GROUPS,
    FILE_GROUPS,
    EXPORT_GROUPS,
    ROW_GROUPS,
    PAGE_GROUPS,
    COLUMN_GROUPS,
    GRID_MORE_GROUPS,
];

fn command_icon(id: &str) -> Option<&'static str> {
    let name = match id {
        "query" => "file-plus",
        "run" => "play",
        "new-connection" => "database",
        "home" => "house",
        "history" => "clock",
        "audit" => "shield",
        "settings" => "settings",
        "font" => "type",
        "theme" => "moon",
        "sidebar" => "panel-left",
        "open-sql-file" => "folder",
        "export-file" | "export-connections" => "download",
        "refresh-table" => "refresh-cw",
        "insert-row" => "plus",
        "delete-rows" => "trash-2",
        "jump-column" => "search",
        "compare" => "git-compare",
        "show-hidden-columns" => "eye",
        _ => return None,
    };
    Some(name)
}

fn command_checked(view: &GridView, id: &str) -> bool {
    match id {
        "sidebar" => view.sidebar.open,
        "results" => view.panel.open,
        "detail" => view.detail_open,
        "theme" => view.dark_mode,
        "wrap-text" => view.wrap_text,
        "continuous" => view.continuous,
        "append-pages" => view.pager.append,
        "show-row-vertically" => view.row_detail_open,
        _ => false,
    }
}

pub(crate) fn command_groups(view: &GridView, ids: &[&[&str]]) -> Vec<Group> {
    let actions = view.palette_actions();
    ids.iter()
        .map(|group| {
            group
                .iter()
                .filter_map(|id| actions.iter().find(|action| action.id == *id))
                .map(|action| Entry {
                    id: action.id,
                    label: action.menu_label().to_string(),
                    enabled: action.enabled,
                    shortcut: view.action_shortcut(action.id),
                    checked: command_checked(view, action.id),
                })
                .collect::<Group>()
        })
        .filter(|group| !group.is_empty())
        .collect()
}

pub(crate) fn set_enabled(groups: &mut [Group], id: &str, enabled: bool) {
    groups
        .iter_mut()
        .flatten()
        .filter(|entry| entry.id == id)
        .for_each(|entry| entry.enabled = entry.enabled && enabled);
}

fn icon_for(name: &str) -> Icon {
    Icon::empty().path(SharedString::from(format!("icons/{name}.svg")))
}

fn row(label: SharedString, shortcut: Option<String>, destructive: bool, cx: &App) -> gpui::AnyElement {
    let theme = Theme::current(cx);
    let colour = if destructive {
        theme.destructive
    } else {
        theme.foreground
    };
    div()
        .id(gpui::ElementId::Name(label.clone()))
        .role(gpui::Role::Label)
        .aria_label(label.clone())
        .flex()
        .flex_1()
        .items_center()
        .justify_between()
        .gap_3()
        .text_color(rgb(colour))
        .child(label)
        .children(shortcut.map(|keys| {
            div()
                .flex_none()
                .text_size(px(size::TEXT_SMALL))
                .text_color(rgb(theme.muted_foreground))
                .child(keys)
        }))
        .test_support()
        .into_any_element()
}

fn entry(
    label: SharedString,
    icon: Option<&'static str>,
    shortcut: Option<String>,
    checked: bool,
    destructive: bool,
) -> PopupMenuItem {
    let item = if shortcut.is_none() && !destructive {
        PopupMenuItem::new(label)
    } else {
        PopupMenuItem::element(move |_, cx| row(label.clone(), shortcut.clone(), destructive, cx))
    };
    item.checked(checked)
        .when_some(icon.filter(|_| !checked), |item, name| item.icon(icon_for(name)))
}

pub(crate) fn item(
    label: impl Into<SharedString>,
    icon: Option<&'static str>,
    shortcut: Option<String>,
    checked: bool,
) -> PopupMenuItem {
    entry(label.into(), icon, shortcut, checked, false)
}

pub(crate) fn destructive_item(
    label: impl Into<SharedString>,
    icon: Option<&'static str>,
    shortcut: Option<String>,
) -> PopupMenuItem {
    entry(label.into(), icon, shortcut, false, true)
}

pub(crate) fn select<T: Clone + PartialEq + 'static>(
    menu: PopupMenu,
    options: &[(SharedString, T)],
    selected: Option<&T>,
    on_pick: impl Fn(T, &mut Window, &mut App) + Clone + 'static,
) -> PopupMenu {
    options.iter().fold(menu, |menu, (label, value)| {
        let (pick, picked) = (on_pick.clone(), value.clone());
        menu.item(
            item(label.clone(), None, None, selected == Some(value))
                .on_click(move |_, window, cx| pick(picked.clone(), window, cx)),
        )
    })
}

fn command_item(view: &WeakEntity<GridView>, entry: &Entry) -> PopupMenuItem {
    let (view, id) = (view.clone(), entry.id);
    let shortcut = entry.shortcut.clone();
    let item = if DESTRUCTIVE.contains(&id) {
        destructive_item(entry.label.clone(), command_icon(id), shortcut)
    } else {
        item(entry.label.clone(), command_icon(id), shortcut, entry.checked)
    };
    item.disabled(!entry.enabled).on_click(move |_, window, cx| {
        let _ = view.update(cx, |this, cx| {
            this.run_palette_id(id, window, cx);
            cx.notify();
        });
    })
}

pub(crate) fn fill(mut menu: PopupMenu, view: &WeakEntity<GridView>, groups: &[Group]) -> PopupMenu {
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            menu = menu.separator();
        }
        for entry in group {
            menu = menu.item(command_item(view, entry));
        }
    }
    menu
}

pub(crate) fn command_menu(
    view: WeakEntity<GridView>,
    groups: Vec<Group>,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |menu, _, _| fill(menu, &view, &groups)
}

pub(crate) struct RowMenu {
    pub position: Point<Pixels>,
    pub menu: Entity<PopupMenu>,
    _dismiss: Subscription,
}

impl RowMenu {
    pub(crate) fn new(position: Point<Pixels>, menu: Entity<PopupMenu>, dismiss: Subscription) -> Self {
        Self {
            position,
            menu,
            _dismiss: dismiss,
        }
    }
}

impl GridView {
    pub(crate) fn open_cell_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let groups = command_groups(self, ROW_GROUPS);
        let owner = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, _, _| fill(menu, &owner, &groups));
        self.show_row_menu(menu, window, cx);
    }

    pub(crate) fn show_row_menu(&mut self, menu: Entity<PopupMenu>, window: &mut Window, cx: &mut Context<Self>) {
        let dismiss = cx.subscribe(&menu, |this, _, _: &DismissEvent, cx| {
            this.row_menu = None;
            cx.notify();
        });
        window.focus(&gpui::Focusable::focus_handle(menu.read(cx), cx), cx);
        self.row_menu = Some(RowMenu::new(window.mouse_position(), menu, dismiss));
        cx.notify();
    }
}

#[cfg(test)]
#[path = "menus_tests.rs"]
mod tests;
