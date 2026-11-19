use gpui::{Context, DismissEvent, Entity, Pixels, Point, Subscription, WeakEntity, Window};
use gpui_component::menu::{PopupMenu, PopupMenuItem};

use crate::GridView;

type Group = Vec<(&'static str, String, bool)>;

pub(crate) const OVERFLOW_GROUPS: &[&[&str]] = &[
    &["query", "run", "explain", "data-dictionary"],
    &["sidebar", "results"],
    &["new-connection", "home", "history", "audit", "settings", "font", "theme"],
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
    &["edit", "delete-rows", "clear-selection"],
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

pub(crate) struct RowMenu {
    pub position: Point<Pixels>,
    pub menu: Entity<PopupMenu>,
    _dismiss: Subscription,
}

impl GridView {
    pub(crate) fn open_cell_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let groups = command_groups(self, ROW_GROUPS);
        let owner = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, _, _| fill(menu, &owner, &groups));
        let dismiss = cx.subscribe(&menu, |this, _, _: &DismissEvent, cx| {
            this.row_menu = None;
            cx.notify();
        });
        window.focus(&gpui::Focusable::focus_handle(menu.read(cx), cx), cx);
        self.row_menu = Some(RowMenu {
            position: window.mouse_position(),
            menu,
            _dismiss: dismiss,
        });
        cx.notify();
    }
}
