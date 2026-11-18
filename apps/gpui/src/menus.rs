use gpui::{Context, WeakEntity, Window};
use gpui_component::menu::{PopupMenu, PopupMenuItem};

use crate::GridView;

type Group = Vec<(&'static str, String, bool)>;

pub(crate) const OVERFLOW_GROUPS: &[&[&str]] = &[
    &["query", "run", "explain"],
    &["sidebar", "results"],
    &["home", "settings", "font", "theme"],
    &["status", "server-status"],
];

pub(crate) const ROW_GROUPS: &[&[&str]] = &[
    &[
        "copy",
        "copy-csv",
        "copy-json",
        "copy-markdown",
        "copy-insert",
        "copy-in-list",
    ],
    &["edit", "clear-selection"],
];

pub(crate) fn command_groups(view: &GridView, ids: &[&[&str]]) -> Vec<Group> {
    let actions = view.palette_actions();
    ids.iter()
        .map(|group| {
            group
                .iter()
                .filter_map(|id| actions.iter().find(|action| action.id == *id))
                .map(|action| (action.id, action.label.clone(), action.enabled))
                .collect::<Group>()
        })
        .filter(|group| !group.is_empty())
        .collect()
}

pub(crate) fn fill(mut menu: PopupMenu, view: &WeakEntity<GridView>, groups: &[Group]) -> PopupMenu {
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            menu = menu.separator();
        }
        for (id, label, enabled) in group {
            let (view, id) = (view.clone(), *id);
            menu = menu.item(
                PopupMenuItem::new(label.clone())
                    .disabled(!*enabled)
                    .on_click(move |_, window, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.run_palette_id(id, window, cx);
                            cx.notify();
                        });
                    }),
            );
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
