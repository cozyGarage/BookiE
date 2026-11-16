use std::cell::{Cell, RefCell};
use std::rc::Rc;

use bookie_app_services::highlight_match::{
    HIGHLIGHT_COLORS, HighlightOp, HighlightRule, HighlightRuleSet, HighlightTarget,
};
use bookie_app_services::highlight_settings::{
    HighlightSettingsStore, default_highlight_rule, normalize_highlight_rules, validate_highlight_pattern_lengths,
};
use bookie_core::FilterValue;
use gtk4::{self as gtk, gio};
use libadwaita::{self as adw, prelude::*};

use super::GridMsg;

type Rebuild = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

#[derive(Clone)]
pub(crate) struct HighlightPersist {
    pub connection_id: uuid::Uuid,
    pub schema: Option<String>,
    pub table: String,
    pub store: HighlightSettingsStore,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueShape {
    None,
    Single,
    Pair,
    List,
}

struct OpEntry {
    op: HighlightOp,
    label: &'static str,
    shape: ValueShape,
}

const OPS: &[OpEntry] = &[
    OpEntry {
        op: HighlightOp::Eq,
        label: "equals",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: HighlightOp::NotEq,
        label: "doesn't equal",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: HighlightOp::Contains,
        label: "contains",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: HighlightOp::StartsWith,
        label: "starts with",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: HighlightOp::EndsWith,
        label: "ends with",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: HighlightOp::Like,
        label: "LIKE",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: HighlightOp::NotLike,
        label: "NOT LIKE",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: HighlightOp::Ilike,
        label: "ILIKE",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: HighlightOp::Lt,
        label: "less than",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: HighlightOp::LtEq,
        label: "less or equal",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: HighlightOp::Gt,
        label: "greater than",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: HighlightOp::GtEq,
        label: "greater or equal",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: HighlightOp::Between,
        label: "between",
        shape: ValueShape::Pair,
    },
    OpEntry {
        op: HighlightOp::In,
        label: "is one of",
        shape: ValueShape::List,
    },
    OpEntry {
        op: HighlightOp::NotIn,
        label: "is none of",
        shape: ValueShape::List,
    },
    OpEntry {
        op: HighlightOp::IsEmpty,
        label: "is empty",
        shape: ValueShape::None,
    },
    OpEntry {
        op: HighlightOp::IsNotEmpty,
        label: "is not empty",
        shape: ValueShape::None,
    },
    OpEntry {
        op: HighlightOp::IsNull,
        label: "is null",
        shape: ValueShape::None,
    },
    OpEntry {
        op: HighlightOp::IsNotNull,
        label: "is not null",
        shape: ValueShape::None,
    },
];

fn shape_for(op: HighlightOp) -> ValueShape {
    OPS.iter()
        .find(|entry| entry.op == op)
        .map(|entry| entry.shape)
        .unwrap_or(ValueShape::Single)
}

fn value_for_shape(shape: ValueShape) -> Option<FilterValue> {
    match shape {
        ValueShape::None => None,
        ValueShape::Single => Some(FilterValue::Single(String::new())),
        ValueShape::Pair => Some(FilterValue::Pair(String::new(), String::new())),
        ValueShape::List => Some(FilterValue::List(Vec::new())),
    }
}

pub(super) fn install(
    view: &gtk::ColumnView,
    actions: &gio::SimpleActionGroup,
    columns: Vec<String>,
    persist: Option<HighlightPersist>,
    applied: Option<relm4::Sender<GridMsg>>,
) {
    let action = gio::SimpleAction::new("highlight-rules", None);
    action.set_enabled(persist.is_some() && !columns.is_empty());
    let weak_view = view.downgrade();
    let columns = Rc::new(columns);
    let persist = Rc::new(persist);
    let applied = Rc::new(applied);
    action.connect_activate(move |_, _| {
        let Some(view) = weak_view.upgrade().filter(|view| view.is_mapped()) else {
            return;
        };
        let Some(persist) = persist.as_ref().clone() else {
            return;
        };
        present(&view, columns.clone(), persist, applied.as_ref().clone());
    });
    actions.add_action(&action);
}

fn present(
    view: &gtk::ColumnView,
    columns: Rc<Vec<String>>,
    persist: HighlightPersist,
    applied: Option<relm4::Sender<GridMsg>>,
) {
    let initial = persist
        .store
        .load(persist.connection_id, persist.schema.as_deref(), &persist.table);
    let original = initial.clone();
    let state = Rc::new(RefCell::new(initial));
    let (dialog, rules_list, add, cancel, apply, status) = build_shell();
    let suppress = Rc::new(Cell::new(false));
    let rebuild = install_rebuild(state.clone(), columns.clone(), rules_list, suppress);
    if let Some(run) = rebuild.borrow().as_ref() {
        run();
    }
    wire_add(add, state.clone(), columns, rebuild.clone());
    wire_cancel(cancel, &dialog);
    wire_apply(apply, &dialog, state, original, status, persist, applied);
    crate::ui::leak_probe::present_tracked("highlight_rules", &dialog, view);
}

fn build_shell() -> (
    adw::Dialog,
    gtk::ListBox,
    gtk::Button,
    gtk::Button,
    gtk::Button,
    gtk::Label,
) {
    let dialog = adw::Dialog::builder()
        .title(bookie_app_services::tr!("Highlight Rules"))
        .content_width(640)
        .content_height(520)
        .build();
    let header = adw::HeaderBar::builder()
        .show_start_title_buttons(false)
        .show_end_title_buttons(false)
        .build();
    let cancel = gtk::Button::with_label(&bookie_app_services::tr!("Cancel"));
    let apply = gtk::Button::with_label(&bookie_app_services::tr!("Apply"));
    apply.add_css_class("suggested-action");
    header.pack_start(&cancel);
    header.pack_end(&apply);
    let rules_list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    let add = gtk::Button::builder()
        .label(bookie_app_services::tr!("Add rule"))
        .halign(gtk::Align::Start)
        .build();
    add.add_css_class("flat");
    let description = gtk::Label::builder()
        .label(bookie_app_services::tr!(
            "LIKE patterns are limited to 4 KiB (4,096 bytes)."
        ))
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();
    let status = gtk::Label::builder().halign(gtk::Align::Start).wrap(true).build();
    status.set_visible(false);
    let body = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    body.append(&description);
    body.append(&status);
    body.append(&rules_list);
    body.append(&add);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&body)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scroller));
    dialog.set_child(Some(&toolbar));
    dialog.set_default_widget(Some(&apply));
    (dialog, rules_list, add, cancel, apply, status)
}

fn install_rebuild(
    state: Rc<RefCell<HighlightRuleSet>>,
    columns: Rc<Vec<String>>,
    rules_list: gtk::ListBox,
    suppress: Rc<Cell<bool>>,
) -> Rebuild {
    let rebuild: Rebuild = Rc::new(RefCell::new(None));
    let slot = rebuild.clone();
    let closure: Rc<dyn Fn()> = Rc::new(move || {
        suppress.set(true);
        while let Some(child) = rules_list.first_child() {
            rules_list.remove(&child);
        }
        let snapshot = state.borrow().rules.clone();
        for (index, rule) in snapshot.iter().enumerate() {
            rules_list.append(&build_rule_row(
                index,
                rule,
                &columns,
                state.clone(),
                slot.clone(),
                suppress.clone(),
            ));
        }
        rules_list.set_visible(!snapshot.is_empty());
        suppress.set(false);
    });
    *rebuild.borrow_mut() = Some(closure);
    rebuild
}

fn wire_add(add: gtk::Button, state: Rc<RefCell<HighlightRuleSet>>, columns: Rc<Vec<String>>, rebuild: Rebuild) {
    add.connect_clicked(move |_| {
        let column = columns.first().cloned().unwrap_or_default();
        state.borrow_mut().rules.push(default_highlight_rule(&column));
        if let Some(run) = rebuild.borrow().as_ref() {
            run();
        }
    });
}

fn wire_cancel(cancel: gtk::Button, dialog: &adw::Dialog) {
    let dialog = dialog.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(dialog) = dialog.upgrade() {
            dialog.close();
        }
    });
}

fn wire_apply(
    apply: gtk::Button,
    dialog: &adw::Dialog,
    state: Rc<RefCell<HighlightRuleSet>>,
    original: HighlightRuleSet,
    status: gtk::Label,
    persist: HighlightPersist,
    applied: Option<relm4::Sender<GridMsg>>,
) {
    let dialog = dialog.downgrade();
    if validate_highlight_pattern_lengths(&HighlightRuleSet::default(), &original).is_err() {
        status.set_text(&bookie_app_services::tr!(
            "An existing LIKE pattern exceeds 4 KiB (4,096 bytes) and is not applied. Edit or remove it."
        ));
        status.add_css_class("warning");
        status.set_visible(true);
    }
    apply.connect_clicked(move |_| {
        let set = normalize_highlight_rules(&state.borrow().rules);
        if validate_highlight_pattern_lengths(&original, &set).is_err() {
            status.set_text(&bookie_app_services::tr!(
                "New LIKE patterns must be 4 KiB (4,096 bytes) or less."
            ));
            status.remove_css_class("warning");
            status.add_css_class("error");
            status.set_visible(true);
            return;
        }
        if let Err(error) = persist
            .store
            .save(persist.connection_id, persist.schema.as_deref(), &persist.table, set)
        {
            if let Some(sender) = &applied {
                sender.send(GridMsg::ShowToast(error)).ok();
            }
            return;
        }
        if let Err(error) = persist.store.flush() {
            tracing::warn!(%error, "highlight rules were saved but not flushed");
        }
        if let Some(sender) = &applied {
            sender.send(GridMsg::HighlightRulesChanged).ok();
        }
        if let Some(dialog) = dialog.upgrade() {
            dialog.close();
        }
    });
}

fn build_rule_row(
    index: usize,
    rule: &HighlightRule,
    columns: &Rc<Vec<String>>,
    state: Rc<RefCell<HighlightRuleSet>>,
    rebuild: Rebuild,
    suppress: Rc<Cell<bool>>,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder().build();
    row.add_prefix(&column_dropdown(
        index,
        rule,
        columns,
        state.clone(),
        rebuild.clone(),
        suppress.clone(),
    ));
    row.add_suffix(&op_dropdown(
        index,
        rule,
        state.clone(),
        rebuild.clone(),
        suppress.clone(),
    ));
    append_value_widgets(&row, index, rule, state.clone(), suppress.clone());
    row.add_suffix(&color_dropdown(index, rule, state.clone(), suppress.clone()));
    row.add_suffix(&target_dropdown(index, rule, state.clone(), suppress.clone()));
    row.add_suffix(&remove_button(index, state, rebuild));
    row
}

fn column_dropdown(
    index: usize,
    rule: &HighlightRule,
    columns: &Rc<Vec<String>>,
    state: Rc<RefCell<HighlightRuleSet>>,
    rebuild: Rebuild,
    suppress: Rc<Cell<bool>>,
) -> gtk::DropDown {
    let labels: Vec<&str> = columns.iter().map(String::as_str).collect();
    let dropdown = gtk::DropDown::from_strings(&labels);
    dropdown.set_valign(gtk::Align::Center);
    let selected = columns.iter().position(|name| name == &rule.column).unwrap_or(0) as u32;
    dropdown.set_selected(selected);
    let columns = columns.clone();
    dropdown.connect_selected_notify(move |dropdown| {
        if suppress.get() {
            return;
        }
        let Some(name) = columns.get(dropdown.selected() as usize) else {
            return;
        };
        if let Some(rule) = state.borrow_mut().rules.get_mut(index) {
            rule.column = name.clone();
        }
        if let Some(run) = rebuild.borrow().as_ref() {
            run();
        }
    });
    dropdown
}

fn op_dropdown(
    index: usize,
    rule: &HighlightRule,
    state: Rc<RefCell<HighlightRuleSet>>,
    rebuild: Rebuild,
    suppress: Rc<Cell<bool>>,
) -> gtk::DropDown {
    let labels: Vec<&str> = OPS.iter().map(|entry| entry.label).collect();
    let dropdown = gtk::DropDown::from_strings(&labels);
    dropdown.set_valign(gtk::Align::Center);
    let selected = OPS.iter().position(|entry| entry.op == rule.op).unwrap_or(0) as u32;
    dropdown.set_selected(selected);
    dropdown.connect_selected_notify(move |dropdown| {
        if suppress.get() {
            return;
        }
        let Some(entry) = OPS.get(dropdown.selected() as usize) else {
            return;
        };
        if let Some(rule) = state.borrow_mut().rules.get_mut(index) {
            rule.op = entry.op;
            rule.value = value_for_shape(entry.shape);
        }
        if let Some(run) = rebuild.borrow().as_ref() {
            run();
        }
    });
    dropdown
}

fn append_value_widgets(
    row: &adw::ActionRow,
    index: usize,
    rule: &HighlightRule,
    state: Rc<RefCell<HighlightRuleSet>>,
    suppress: Rc<Cell<bool>>,
) {
    match shape_for(rule.op) {
        ValueShape::None => {}
        ValueShape::Single => row.add_suffix(&single_value_entry(index, rule, state, suppress)),
        ValueShape::Pair => row.add_suffix(&pair_value_box(index, rule, state, suppress)),
        ValueShape::List => row.add_suffix(&list_value_entry(index, rule, state, suppress)),
    }
}

fn single_value_entry(
    index: usize,
    rule: &HighlightRule,
    state: Rc<RefCell<HighlightRuleSet>>,
    suppress: Rc<Cell<bool>>,
) -> gtk::Entry {
    let entry = gtk::Entry::builder()
        .placeholder_text(bookie_app_services::tr!("Value"))
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build();
    if let Some(FilterValue::Single(text)) = rule.value.as_ref() {
        entry.set_text(text);
    }
    entry.connect_changed(move |entry| {
        if suppress.get() {
            return;
        }
        if let Some(rule) = state.borrow_mut().rules.get_mut(index) {
            rule.value = Some(FilterValue::Single(entry.text().to_string()));
        }
    });
    entry
}

fn pair_value_box(
    index: usize,
    rule: &HighlightRule,
    state: Rc<RefCell<HighlightRuleSet>>,
    suppress: Rc<Cell<bool>>,
) -> gtk::Box {
    let lo = gtk::Entry::builder()
        .placeholder_text(bookie_app_services::tr!("From"))
        .valign(gtk::Align::Center)
        .build();
    let hi = gtk::Entry::builder()
        .placeholder_text(bookie_app_services::tr!("To"))
        .valign(gtk::Align::Center)
        .build();
    if let Some(FilterValue::Pair(a, b)) = rule.value.as_ref() {
        lo.set_text(a);
        hi.set_text(b);
    }
    {
        let state = state.clone();
        let suppress = suppress.clone();
        let hi = hi.clone();
        lo.connect_changed(move |entry| {
            if suppress.get() {
                return;
            }
            if let Some(rule) = state.borrow_mut().rules.get_mut(index) {
                rule.value = Some(FilterValue::Pair(entry.text().to_string(), hi.text().to_string()));
            }
        });
    }
    {
        let lo = lo.clone();
        hi.connect_changed(move |entry| {
            if suppress.get() {
                return;
            }
            if let Some(rule) = state.borrow_mut().rules.get_mut(index) {
                rule.value = Some(FilterValue::Pair(lo.text().to_string(), entry.text().to_string()));
            }
        });
    }
    let pair = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    pair.append(&lo);
    pair.append(&hi);
    pair
}

fn list_value_entry(
    index: usize,
    rule: &HighlightRule,
    state: Rc<RefCell<HighlightRuleSet>>,
    suppress: Rc<Cell<bool>>,
) -> gtk::Entry {
    let entry = gtk::Entry::builder()
        .placeholder_text(bookie_app_services::tr!("a, b, c"))
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build();
    if let Some(FilterValue::List(items)) = rule.value.as_ref() {
        entry.set_text(&items.join(", "));
    }
    entry.connect_changed(move |entry| {
        if suppress.get() {
            return;
        }
        if let Some(rule) = state.borrow_mut().rules.get_mut(index) {
            let items: Vec<String> = entry
                .text()
                .split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect();
            rule.value = Some(FilterValue::List(items));
        }
    });
    entry
}

fn color_dropdown(
    index: usize,
    rule: &HighlightRule,
    state: Rc<RefCell<HighlightRuleSet>>,
    suppress: Rc<Cell<bool>>,
) -> gtk::DropDown {
    let dropdown = gtk::DropDown::from_strings(HIGHLIGHT_COLORS);
    dropdown.set_valign(gtk::Align::Center);
    let selected = HIGHLIGHT_COLORS
        .iter()
        .position(|color| *color == rule.color)
        .unwrap_or(0) as u32;
    dropdown.set_selected(selected);
    dropdown.connect_selected_notify(move |dropdown| {
        if suppress.get() {
            return;
        }
        let Some(color) = HIGHLIGHT_COLORS.get(dropdown.selected() as usize) else {
            return;
        };
        if let Some(rule) = state.borrow_mut().rules.get_mut(index) {
            rule.color = (*color).to_string();
        }
    });
    dropdown
}

fn target_dropdown(
    index: usize,
    rule: &HighlightRule,
    state: Rc<RefCell<HighlightRuleSet>>,
    suppress: Rc<Cell<bool>>,
) -> gtk::DropDown {
    let dropdown = gtk::DropDown::from_strings(&[&bookie_app_services::tr!("Cell"), &bookie_app_services::tr!("Row")]);
    dropdown.set_valign(gtk::Align::Center);
    dropdown.set_selected(match rule.target {
        HighlightTarget::Cell => 0,
        HighlightTarget::Row => 1,
    });
    dropdown.connect_selected_notify(move |dropdown| {
        if suppress.get() {
            return;
        }
        if let Some(rule) = state.borrow_mut().rules.get_mut(index) {
            rule.target = if dropdown.selected() == 1 {
                HighlightTarget::Row
            } else {
                HighlightTarget::Cell
            };
        }
    });
    dropdown
}

fn remove_button(index: usize, state: Rc<RefCell<HighlightRuleSet>>, rebuild: Rebuild) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text(bookie_app_services::tr!("Remove rule"))
        .valign(gtk::Align::Center)
        .build();
    button.add_css_class("flat");
    button.connect_clicked(move |_| {
        let mut set = state.borrow_mut();
        if index < set.rules.len() {
            set.rules.remove(index);
        }
        drop(set);
        if let Some(run) = rebuild.borrow().as_ref() {
            run();
        }
    });
    button
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_app_services::highlight_match::MAX_LIKE_PATTERN_BYTES;

    fn long_like_rule() -> HighlightRule {
        let mut rule = default_highlight_rule("status");
        rule.op = HighlightOp::Like;
        rule.value = Some(FilterValue::Single("x".repeat(MAX_LIKE_PATTERN_BYTES + 1)));
        rule
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn a_new_over_limit_pattern_is_refused_without_saving_or_closing() {
        gtk::init().expect("GTK display");
        let dir = tempfile::tempdir().expect("settings directory");
        let store = HighlightSettingsStore::load_in(dir.path());
        let connection_id = uuid::Uuid::new_v4();
        let (dialog, _, _, _, apply, status) = build_shell();
        let window = gtk::Window::new();
        window.present();
        dialog.present(Some(&window));
        wire_apply(
            apply.clone(),
            &dialog,
            Rc::new(RefCell::new(HighlightRuleSet {
                rules: vec![long_like_rule()],
            })),
            HighlightRuleSet::default(),
            status.clone(),
            HighlightPersist {
                connection_id,
                schema: None,
                table: "orders".into(),
                store: store.clone(),
            },
            None,
        );
        apply.emit_clicked();
        assert!(dialog.is_visible());
        assert!(status.is_visible());
        assert!(store.load(connection_id, None, "orders").is_empty());
        dialog.close();
        window.destroy();
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn an_unchanged_legacy_pattern_is_warned_and_survives_unrelated_changes() {
        gtk::init().expect("GTK display");
        let dir = tempfile::tempdir().expect("settings directory");
        let store = HighlightSettingsStore::load_in(dir.path());
        let connection_id = uuid::Uuid::new_v4();
        let original = HighlightRuleSet {
            rules: vec![long_like_rule()],
        };
        let mut candidate = original.clone();
        candidate.rules.push(default_highlight_rule("kind"));
        let (dialog, _, _, _, apply, status) = build_shell();
        let window = gtk::Window::new();
        window.present();
        dialog.present(Some(&window));
        wire_apply(
            apply.clone(),
            &dialog,
            Rc::new(RefCell::new(candidate.clone())),
            original.clone(),
            status.clone(),
            HighlightPersist {
                connection_id,
                schema: None,
                table: "orders".into(),
                store: store.clone(),
            },
            None,
        );
        assert!(status.is_visible());
        apply.emit_clicked();
        while gtk::glib::MainContext::default().iteration(false) {}
        assert_eq!(
            store.load(connection_id, None, "orders"),
            normalize_highlight_rules(&candidate.rules)
        );
        window.destroy();
    }
}
