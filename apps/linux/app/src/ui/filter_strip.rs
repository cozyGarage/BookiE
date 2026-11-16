use std::cell::{Cell, RefCell};
use std::rc::Rc;

use relm4::adw::prelude::*;
use relm4::{adw, gtk};

use bookie_core::{
    ColumnInfo, Combinator, FilterOp, FilterRule, FilterSet, FilterValue, FilterValueKind, filter_value_kind,
};

type Rebuilder = Rc<dyn Fn()>;
type RebuilderSlot = Rc<RefCell<Option<Rebuilder>>>;

fn extra_is_blank(extra: Option<&str>) -> bool {
    extra.map(|s| s.trim().is_empty()).unwrap_or(true)
}

struct OpEntry {
    op: FilterOp,
    label: &'static str,
    shape: ValueShape,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueShape {
    None,
    Single,
    Pair,
    List,
}

fn operators_for(column: &ColumnInfo) -> &'static [OpEntry] {
    match filter_value_kind(column) {
        FilterValueKind::Bool => &OPS_BOOL,
        FilterValueKind::Uuid | FilterValueKind::Json => &OPS_UUID,
        FilterValueKind::Int
        | FilterValueKind::Float
        | FilterValueKind::Decimal
        | FilterValueKind::Date
        | FilterValueKind::Time
        | FilterValueKind::DateTime
        | FilterValueKind::TimestampTz => &OPS_NUMERIC,
        FilterValueKind::Text | FilterValueKind::Bytes => &OPS_TEXT,
    }
}

const OPS_TEXT: [OpEntry; 14] = [
    OpEntry {
        op: FilterOp::Eq,
        label: "equals",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::NotEq,
        label: "doesn't equal",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::Contains,
        label: "contains",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::StartsWith,
        label: "starts with",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::EndsWith,
        label: "ends with",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::Like,
        label: "LIKE",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::NotLike,
        label: "NOT LIKE",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::Ilike,
        label: "ILIKE (case-insensitive)",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::IsNull,
        label: "is empty",
        shape: ValueShape::None,
    },
    OpEntry {
        op: FilterOp::IsNotNull,
        label: "is not empty",
        shape: ValueShape::None,
    },
    OpEntry {
        op: FilterOp::In,
        label: "is one of",
        shape: ValueShape::List,
    },
    OpEntry {
        op: FilterOp::NotIn,
        label: "is none of",
        shape: ValueShape::List,
    },
    OpEntry {
        op: FilterOp::Lt,
        label: "less than (lex)",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::Gt,
        label: "greater than (lex)",
        shape: ValueShape::Single,
    },
];

const OPS_NUMERIC: [OpEntry; 11] = [
    OpEntry {
        op: FilterOp::Eq,
        label: "=",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::NotEq,
        label: "≠",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::Lt,
        label: "<",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::LtEq,
        label: "≤",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::Gt,
        label: ">",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::GtEq,
        label: "≥",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::Between,
        label: "between",
        shape: ValueShape::Pair,
    },
    OpEntry {
        op: FilterOp::IsNull,
        label: "is empty",
        shape: ValueShape::None,
    },
    OpEntry {
        op: FilterOp::IsNotNull,
        label: "is not empty",
        shape: ValueShape::None,
    },
    OpEntry {
        op: FilterOp::In,
        label: "is one of",
        shape: ValueShape::List,
    },
    OpEntry {
        op: FilterOp::NotIn,
        label: "is none of",
        shape: ValueShape::List,
    },
];

const OPS_BOOL: [OpEntry; 3] = [
    OpEntry {
        op: FilterOp::Eq,
        label: "=",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::IsNull,
        label: "is empty",
        shape: ValueShape::None,
    },
    OpEntry {
        op: FilterOp::IsNotNull,
        label: "is not empty",
        shape: ValueShape::None,
    },
];

const OPS_UUID: [OpEntry; 4] = [
    OpEntry {
        op: FilterOp::Eq,
        label: "=",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::NotEq,
        label: "≠",
        shape: ValueShape::Single,
    },
    OpEntry {
        op: FilterOp::IsNull,
        label: "is empty",
        shape: ValueShape::None,
    },
    OpEntry {
        op: FilterOp::IsNotNull,
        label: "is not empty",
        shape: ValueShape::None,
    },
];

// Byte values have no meaningful text comparison in this editor.
fn is_filterable(col: &ColumnInfo) -> bool {
    let lower = col.data_type.to_ascii_lowercase();
    !(lower.contains("bytea") || lower.contains("blob"))
}

pub struct FilterStrip {
    pub widget: gtk::Revealer,
    state: Rc<RefCell<FilterSet>>,
    columns: Rc<RefCell<Vec<ColumnInfo>>>,
    rebuild: RebuilderSlot,
    raw_entry: gtk::Entry,
    rules_expander: gtk::Expander,
}

impl FilterStrip {
    pub fn is_revealed(&self) -> bool {
        self.widget.reveals_child()
    }

    pub fn set_revealed(&self, revealed: bool) {
        self.widget.set_reveal_child(revealed);
    }

    pub fn toggle(&self) {
        let opening = !self.is_revealed();
        self.set_revealed(opening);
        if opening {
            // Focus the primary SQL field when the strip opens.
            self.raw_entry.grab_focus();
        }
    }

    pub fn update_columns(&self, columns: Vec<ColumnInfo>) {
        *self.columns.borrow_mut() = columns.into_iter().filter(is_filterable).collect();
        if let Some(f) = self.rebuild.borrow().as_ref() {
            f();
        }
    }

    pub fn update_filter(&self, set: FilterSet) {
        let extra = set.extra_sql.clone().unwrap_or_default();
        self.rules_expander.set_expanded(!set.rules.is_empty());
        *self.state.borrow_mut() = set;
        self.raw_entry.set_text(&extra);
        if let Some(f) = self.rebuild.borrow().as_ref() {
            f();
        }
    }
}

#[derive(Clone)]
struct StripState {
    filters: Rc<RefCell<FilterSet>>,
    columns: Rc<RefCell<Vec<ColumnInfo>>>,
    // GTK re-emits input signals during rebuilds; handlers ignore them while this is set.
    suppress: Rc<Cell<bool>>,
    rebuild: RebuilderSlot,
}

impl StripState {
    fn new(initial: FilterSet, columns: Vec<ColumnInfo>) -> Self {
        Self {
            filters: Rc::new(RefCell::new(initial)),
            columns: Rc::new(RefCell::new(columns.into_iter().filter(is_filterable).collect())),
            suppress: Rc::new(Cell::new(false)),
            rebuild: Rc::new(RefCell::new(None)),
        }
    }

    fn rebuild_rules(&self) {
        if let Some(rebuild) = self.rebuild.borrow().as_ref() {
            rebuild();
        }
    }
}

struct ActionButtons {
    clear: gtk::Button,
    apply: gtk::Button,
    close: gtk::Button,
}

pub fn build(
    columns: Vec<ColumnInfo>,
    initial: FilterSet,
    on_apply: Rc<dyn Fn(FilterSet)>,
    preset_context: Option<crate::ui::filter_presets::FilterPresetContext>,
) -> FilterStrip {
    let state = StripState::new(initial, columns);
    let revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .reveal_child(false)
        .build();
    let outer = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(0)
        .build();
    outer.add_css_class("toolbar");
    outer.add_css_class("inline-toolbar");
    revealer.set_child(Some(&outer));

    let match_revealer = build_match_controls(&state);
    let buttons = build_action_buttons();
    let (where_row, raw_entry) = build_where_row(&state, &buttons.apply);
    let saved_filters = preset_context.map(|context| build_saved_filters_button(context, &state, &on_apply));
    outer.append(&build_header(&where_row, &buttons, saved_filters));
    let rules_expander = build_rules_section(&state, &match_revealer);
    outer.append(&build_scroller(&rules_expander));
    wire_strip_actions(&buttons, &revealer, &state, on_apply);
    install_escape_close(&outer, &revealer);

    FilterStrip {
        widget: revealer,
        state: state.filters,
        columns: state.columns,
        rebuild: state.rebuild,
        raw_entry,
        rules_expander,
    }
}

fn build_match_controls(state: &StripState) -> gtk::Revealer {
    let match_label = gtk::Label::builder().label(bookie_app_services::tr!("Match")).build();
    match_label.add_css_class("dim-label");
    let combinator_dropdown =
        gtk::DropDown::from_strings(&[&bookie_app_services::tr!("all"), &bookie_app_services::tr!("any")]);
    combinator_dropdown.set_valign(gtk::Align::Center);
    combinator_dropdown.set_selected(match state.filters.borrow().combinator {
        Combinator::And => 0,
        Combinator::Or => 1,
    });
    let match_suffix = gtk::Label::builder()
        .label(bookie_app_services::tr!("of these rules"))
        .build();
    match_suffix.add_css_class("dim-label");
    let match_row_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    match_row_box.append(&match_label);
    match_row_box.append(&combinator_dropdown);
    match_row_box.append(&match_suffix);
    let filters = state.filters.clone();
    combinator_dropdown.connect_selected_notify(move |dd| {
        filters.borrow_mut().combinator = match dd.selected() {
            1 => Combinator::Or,
            _ => Combinator::And,
        };
    });
    gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::None)
        .reveal_child(state.filters.borrow().rules.len() >= 2)
        .child(&match_row_box)
        .build()
}

fn build_action_buttons() -> ActionButtons {
    let clear = gtk::Button::with_label(&bookie_app_services::tr!("Clear all"));
    clear.add_css_class("flat");
    let apply = gtk::Button::with_label(&bookie_app_services::tr!("Apply"));
    apply.add_css_class("suggested-action");
    let close = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text(bookie_app_services::tr!("Close (Esc)"))
        .build();
    close.add_css_class("flat");
    ActionButtons { clear, apply, close }
}

fn build_where_row(state: &StripState, apply_btn: &gtk::Button) -> (gtk::Box, gtk::Entry) {
    let where_label = gtk::Label::builder().label("WHERE").build();
    where_label.add_css_class("monospace");
    where_label.add_css_class("dim-label");
    let raw_entry = gtk::Entry::builder()
        .placeholder_text(bookie_app_services::tr!("e.g. created_at > now() - interval '1 day'"))
        .hexpand(true)
        .build();
    raw_entry.add_css_class("monospace");
    raw_entry.set_text(state.filters.borrow().extra_sql.as_deref().unwrap_or(""));
    let strip = state.clone();
    raw_entry.connect_changed(move |e| {
        let text = e.text().to_string();
        let trimmed = text.trim();
        strip.filters.borrow_mut().extra_sql = if trimmed.is_empty() { None } else { Some(text) };
        strip.rebuild_rules();
    });
    let apply_btn = apply_btn.clone();
    raw_entry.connect_activate(move |_| {
        apply_btn.activate();
    });
    let where_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .hexpand(true)
        .build();
    where_row.append(&where_label);
    where_row.append(&raw_entry);
    (where_row, raw_entry)
}

fn build_saved_filters_button(
    context: crate::ui::filter_presets::FilterPresetContext,
    state: &StripState,
    on_apply: &Rc<dyn Fn(FilterSet)>,
) -> gtk::MenuButton {
    let presets =
        crate::ui::filter_presets::build(context, state.filters.clone(), state.columns.clone(), on_apply.clone());
    let popover = gtk::Popover::new();
    popover.set_child(Some(&presets.widget));
    let saved = gtk::MenuButton::builder()
        .label(bookie_app_services::tr!("Saved filters"))
        .popover(&popover)
        .build();
    saved.add_css_class("flat");
    saved
}

fn build_header(where_row: &gtk::Box, buttons: &ActionButtons, saved_filters: Option<gtk::MenuButton>) -> adw::WrapBox {
    let header = adw::WrapBox::builder()
        .child_spacing(6)
        .line_spacing(6)
        .margin_top(8)
        .margin_bottom(4)
        .margin_start(12)
        .margin_end(12)
        .build();
    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .halign(gtk::Align::End)
        .build();
    if let Some(saved) = saved_filters {
        actions.append(&saved);
    }
    actions.append(&buttons.clear);
    actions.append(&buttons.apply);
    actions.append(&buttons.close);
    header.append(where_row);
    header.append(&actions);
    header
}

fn build_rules_section(state: &StripState, match_revealer: &gtk::Revealer) -> gtk::Expander {
    let rules_expander = gtk::Expander::builder()
        .label(bookie_app_services::tr!("Or use the rule editor"))
        .expanded(!state.filters.borrow().rules.is_empty())
        .build();
    let rules_body = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .margin_top(8)
        .build();
    rules_body.append(match_revealer);
    rules_expander.set_child(Some(&rules_body));
    let rules_list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).build();
    rules_list.add_css_class("boxed-list");
    rules_body.append(&rules_list);
    let add_rule_btn = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .label(bookie_app_services::tr!("Add rule"))
        .halign(gtk::Align::Start)
        .build();
    add_rule_btn.add_css_class("flat");
    rules_body.append(&add_rule_btn);

    install_rebuilder(state, &rules_list, match_revealer);
    state.rebuild_rules();
    connect_add_rule(state, &add_rule_btn);
    rules_expander
}

fn install_rebuilder(state: &StripState, rules_list: &gtk::ListBox, match_revealer: &gtk::Revealer) {
    let strip = state.clone();
    let rules_list = rules_list.clone();
    let match_revealer = match_revealer.clone();
    let closure: Rebuilder = Rc::new(move || {
        strip.suppress.set(true);
        while let Some(child) = rules_list.first_child() {
            rules_list.remove(&child);
        }
        let rules_snapshot = strip.filters.borrow().rules.clone();
        for (i, rule) in rules_snapshot.iter().enumerate() {
            rules_list.append(&build_rule_row(i, rule, &strip));
        }
        let raw_present = !extra_is_blank(strip.filters.borrow().extra_sql.as_deref());
        let needs_combinator = rules_snapshot.len() >= 2 || (!rules_snapshot.is_empty() && raw_present);
        match_revealer.set_reveal_child(needs_combinator);
        rules_list.set_visible(!rules_snapshot.is_empty());
        strip.suppress.set(false);
    });
    *state.rebuild.borrow_mut() = Some(closure);
}

fn connect_add_rule(state: &StripState, add_rule_btn: &gtk::Button) {
    let strip = state.clone();
    add_rule_btn.connect_clicked(move |_| {
        let default_col = strip
            .columns
            .borrow()
            .first()
            .map(|c| c.name.clone())
            .unwrap_or_default();
        strip.filters.borrow_mut().rules.push(FilterRule {
            column: default_col,
            op: FilterOp::Eq,
            value: Some(FilterValue::Single(String::new())),
        });
        strip.rebuild_rules();
    });
}

fn build_scroller(rules_expander: &gtk::Expander) -> gtk::ScrolledWindow {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .margin_top(6)
        .margin_bottom(8)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(rules_expander);
    gtk::ScrolledWindow::builder()
        .child(&content)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(false)
        .max_content_height(420)
        .propagate_natural_height(true)
        .hexpand(true)
        .build()
}

fn wire_strip_actions(
    buttons: &ActionButtons,
    revealer: &gtk::Revealer,
    state: &StripState,
    on_apply: Rc<dyn Fn(FilterSet)>,
) {
    let revealer_for_close = revealer.clone();
    buttons.close.connect_clicked(move |_| {
        revealer_for_close.set_reveal_child(false);
    });

    let revealer_for_clear = revealer.clone();
    let on_apply_for_clear = on_apply.clone();
    buttons.clear.connect_clicked(move |_| {
        on_apply_for_clear(FilterSet::default());
        revealer_for_clear.set_reveal_child(false);
    });

    let revealer_for_apply = revealer.clone();
    let filters = state.filters.clone();
    buttons.apply.connect_clicked(move |_| {
        let snapshot = filters.borrow().clone();
        on_apply(snapshot);
        revealer_for_apply.set_reveal_child(false);
    });
}

fn install_escape_close(outer: &gtk::Box, revealer: &gtk::Revealer) {
    let revealer_for_esc = revealer.clone();
    let esc_shortcut = gtk::Shortcut::builder()
        .trigger(&crate::ui::shortcut::parse("Escape"))
        .action(&gtk::CallbackAction::new(move |_, _| {
            revealer_for_esc.set_reveal_child(false);
            relm4::gtk::glib::Propagation::Stop
        }))
        .build();
    let esc_controller = gtk::ShortcutController::new();
    esc_controller.set_scope(gtk::ShortcutScope::Local);
    esc_controller.add_shortcut(esc_shortcut);
    outer.add_controller(esc_controller);
}

fn fallback_column(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.to_owned(),
        data_type: "text".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

fn empty_value(shape: ValueShape) -> Option<FilterValue> {
    match shape {
        ValueShape::None => None,
        ValueShape::Single => Some(FilterValue::Single(String::new())),
        ValueShape::Pair => Some(FilterValue::Pair(String::new(), String::new())),
        ValueShape::List => Some(FilterValue::List(Vec::new())),
    }
}

fn retarget_column(rule: &mut FilterRule, column: &ColumnInfo) {
    let first = &operators_for(column)[0];
    rule.column = column.name.clone();
    rule.op = first.op;
    rule.value = empty_value(first.shape);
}

fn retarget_operator(rule: &mut FilterRule, columns: &[ColumnInfo], operator_index: usize) {
    let column = columns
        .iter()
        .find(|c| c.name == rule.column)
        .cloned()
        .unwrap_or_else(|| fallback_column(&rule.column));
    if let Some(entry) = operators_for(&column).get(operator_index) {
        rule.op = entry.op;
        rule.value = empty_value(entry.shape);
    }
}

fn build_rule_row(index: usize, rule: &FilterRule, state: &StripState) -> adw::ActionRow {
    let columns = state.columns.borrow().clone();
    let row = adw::ActionRow::builder().build();
    let selected = columns.iter().position(|c| c.name == rule.column).unwrap_or(0);
    row.add_prefix(&build_column_dropdown(index, &columns, selected, state));
    let column = columns
        .get(selected)
        .cloned()
        .unwrap_or_else(|| fallback_column(&rule.column));
    let ops = operators_for(&column);
    row.add_suffix(&build_operator_dropdown(index, rule, ops, state));
    let shape = ops
        .iter()
        .find(|e| e.op == rule.op)
        .map(|e| e.shape)
        .unwrap_or(ValueShape::Single);
    match shape {
        ValueShape::None => {}
        ValueShape::Single => row.add_suffix(&single_value_entry(index, rule, &column, state)),
        ValueShape::Pair => row.add_suffix(&pair_value_entries(index, rule, &column, state)),
        ValueShape::List => row.add_suffix(&list_value_entry(index, rule, state)),
    }
    row.add_suffix(&build_remove_button(index, state));
    row
}

fn build_column_dropdown(index: usize, columns: &[ColumnInfo], selected: usize, state: &StripState) -> gtk::DropDown {
    let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
    let dropdown = gtk::DropDown::from_strings(&names);
    dropdown.set_valign(gtk::Align::Center);
    dropdown.set_selected(selected as u32);
    let strip = state.clone();
    dropdown.connect_selected_notify(move |dd| {
        if strip.suppress.get() {
            return;
        }
        let cols = strip.columns.borrow();
        let Some(new_col) = cols.get(dd.selected() as usize) else {
            return;
        };
        if let Some(rule) = strip.filters.borrow_mut().rules.get_mut(index) {
            retarget_column(rule, new_col);
        }
        drop(cols);
        strip.rebuild_rules();
    });
    dropdown
}

fn build_operator_dropdown(index: usize, rule: &FilterRule, ops: &[OpEntry], state: &StripState) -> gtk::DropDown {
    let labels: Vec<&str> = ops.iter().map(|e| e.label).collect();
    let dropdown = gtk::DropDown::from_strings(&labels);
    dropdown.set_valign(gtk::Align::Center);
    dropdown.set_selected(ops.iter().position(|e| e.op == rule.op).unwrap_or(0) as u32);
    let strip = state.clone();
    dropdown.connect_selected_notify(move |dd| {
        if strip.suppress.get() {
            return;
        }
        let mut filters = strip.filters.borrow_mut();
        let Some(rule) = filters.rules.get_mut(index) else {
            return;
        };
        retarget_operator(rule, &strip.columns.borrow(), dd.selected() as usize);
        drop(filters);
        strip.rebuild_rules();
    });
    dropdown
}

fn single_value_entry(index: usize, rule: &FilterRule, column: &ColumnInfo, state: &StripState) -> gtk::Entry {
    let entry = gtk::Entry::builder()
        .placeholder_text(bookie_app_services::tr!("Value"))
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build();
    entry.set_input_purpose(input_purpose_for(&column.data_type));
    if let Some(FilterValue::Single(s)) = rule.value.as_ref() {
        entry.set_text(s);
    }
    let strip = state.clone();
    entry.connect_changed(move |e| {
        if strip.suppress.get() {
            return;
        }
        if let Some(rule) = strip.filters.borrow_mut().rules.get_mut(index) {
            rule.value = Some(FilterValue::Single(e.text().to_string()));
        }
    });
    entry
}

fn pair_value_entries(index: usize, rule: &FilterRule, column: &ColumnInfo, state: &StripState) -> gtk::Box {
    let lo = gtk::Entry::builder()
        .placeholder_text(bookie_app_services::tr!("From"))
        .valign(gtk::Align::Center)
        .build();
    let hi = gtk::Entry::builder()
        .placeholder_text(bookie_app_services::tr!("To"))
        .valign(gtk::Align::Center)
        .build();
    lo.set_input_purpose(input_purpose_for(&column.data_type));
    hi.set_input_purpose(input_purpose_for(&column.data_type));
    if let Some(FilterValue::Pair(a, b)) = rule.value.as_ref() {
        lo.set_text(a);
        hi.set_text(b);
    }
    let strip = state.clone();
    let hi_for_lo = hi.clone();
    lo.connect_changed(move |e| {
        if strip.suppress.get() {
            return;
        }
        if let Some(rule) = strip.filters.borrow_mut().rules.get_mut(index) {
            rule.value = Some(FilterValue::Pair(e.text().to_string(), hi_for_lo.text().to_string()));
        }
    });
    let strip = state.clone();
    let lo_for_hi = lo.clone();
    hi.connect_changed(move |e| {
        if strip.suppress.get() {
            return;
        }
        if let Some(rule) = strip.filters.borrow_mut().rules.get_mut(index) {
            rule.value = Some(FilterValue::Pair(lo_for_hi.text().to_string(), e.text().to_string()));
        }
    });
    let pair_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    pair_box.append(&lo);
    pair_box.append(&hi);
    pair_box
}

fn list_value_entry(index: usize, rule: &FilterRule, state: &StripState) -> gtk::Entry {
    let entry = gtk::Entry::builder()
        .placeholder_text(bookie_app_services::tr!("a, b, c"))
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build();
    if let Some(FilterValue::List(items)) = rule.value.as_ref() {
        entry.set_text(&items.join(", "));
    }
    let strip = state.clone();
    entry.connect_changed(move |e| {
        if strip.suppress.get() {
            return;
        }
        if let Some(rule) = strip.filters.borrow_mut().rules.get_mut(index) {
            let items: Vec<String> = e
                .text()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            rule.value = Some(FilterValue::List(items));
        }
    });
    entry
}

fn build_remove_button(index: usize, state: &StripState) -> gtk::Button {
    let remove = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text(bookie_app_services::tr!("Remove rule"))
        .valign(gtk::Align::Center)
        .build();
    remove.add_css_class("flat");
    let strip = state.clone();
    remove.connect_clicked(move |_| {
        let mut filters = strip.filters.borrow_mut();
        if index < filters.rules.len() {
            filters.rules.remove(index);
        }
        drop(filters);
        strip.rebuild_rules();
    });
    remove
}

fn input_purpose_for(data_type: &str) -> gtk::InputPurpose {
    let lower = data_type.to_ascii_lowercase();
    let is_numeric = lower.starts_with("int")
        || lower.starts_with("bigint")
        || lower.starts_with("smallint")
        || lower.starts_with("tinyint")
        || lower.contains("serial")
        || lower.contains("decimal")
        || lower.contains("numeric")
        || lower.contains("double")
        || lower.contains("real")
        || lower.contains("float");
    if is_numeric {
        gtk::InputPurpose::Number
    } else {
        gtk::InputPurpose::FreeForm
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::{
        ColumnInfo, FilterOp, FilterRule, FilterSet, FilterValue, build, operators_for, retarget_column,
        retarget_operator,
    };
    use relm4::{gtk, gtk::prelude::*};

    fn column(data_type: &str) -> ColumnInfo {
        ColumnInfo {
            name: "value".into(),
            data_type: data_type.into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }
    }

    #[test]
    fn integer_and_time_columns_offer_comparisons_instead_of_text_matching() {
        for data_type in [
            "mediumint",
            "mediumint(8) unsigned",
            "int unsigned",
            "time without time zone",
            "time(3) without time zone",
        ] {
            let ops = operators_for(&column(data_type));
            assert!(ops.iter().any(|entry| entry.op == FilterOp::Gt), "{data_type}");
            assert!(!ops.iter().any(|entry| entry.op == FilterOp::Contains), "{data_type}");
        }
    }

    fn named(name: &str, data_type: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            ..column(data_type)
        }
    }

    fn rule(column: &str, op: FilterOp, value: Option<FilterValue>) -> FilterRule {
        FilterRule {
            column: column.into(),
            op,
            value,
        }
    }

    #[test]
    fn moving_a_rule_to_another_column_restarts_it_at_that_columns_first_operator() {
        let mut moved = rule(
            "age",
            FilterOp::Between,
            Some(FilterValue::Pair("1".into(), "5".into())),
        );

        retarget_column(&mut moved, &named("name", "text"));

        assert_eq!(moved.column, "name");
        assert_eq!(moved.op, FilterOp::Eq);
        assert_eq!(moved.value, Some(FilterValue::Single(String::new())));
    }

    #[test]
    fn choosing_an_operator_gives_the_rule_an_empty_value_of_that_operators_shape() {
        let text = [named("name", "text")];
        let number = [named("age", "integer")];
        let cases = [
            (
                &text[..],
                "name",
                2,
                FilterOp::Contains,
                Some(FilterValue::Single(String::new())),
            ),
            (&text[..], "name", 8, FilterOp::IsNull, None),
            (&text[..], "name", 10, FilterOp::In, Some(FilterValue::List(Vec::new()))),
            (
                &number[..],
                "age",
                6,
                FilterOp::Between,
                Some(FilterValue::Pair(String::new(), String::new())),
            ),
        ];
        for (columns, column, index, op, value) in cases {
            let mut chosen = rule(column, FilterOp::Eq, Some(FilterValue::Single("old".into())));

            retarget_operator(&mut chosen, columns, index);

            assert_eq!(chosen.op, op, "operator {index} on {column}");
            assert_eq!(chosen.value, value, "operator {index} on {column}");
        }
    }

    #[test]
    fn an_operator_index_past_the_list_leaves_the_rule_unchanged() {
        let columns = [named("name", "text")];
        let mut kept = rule("name", FilterOp::Contains, Some(FilterValue::Single("abc".into())));

        retarget_operator(&mut kept, &columns, 99);

        assert_eq!(kept.op, FilterOp::Contains);
        assert_eq!(kept.value, Some(FilterValue::Single("abc".into())));
    }

    #[test]
    fn a_rule_naming_a_column_that_is_gone_is_treated_as_text() {
        let mut orphan = rule("ghost", FilterOp::Eq, Some(FilterValue::Single(String::new())));

        retarget_operator(&mut orphan, &[], 2);

        assert_eq!(orphan.op, FilterOp::Contains);
    }

    #[test]
    fn text_columns_keep_text_matching() {
        let ops = operators_for(&column("character varying(20)"));
        assert!(ops.iter().any(|entry| entry.op == FilterOp::Contains));
    }

    #[test]
    #[ignore = "requires isolated GTK display"]
    fn an_externally_applied_rule_expands_the_rule_editor() {
        gtk::init().unwrap();
        let strip = build(Vec::new(), FilterSet::default(), Rc::new(|_| {}), None);
        let widget: gtk::Widget = strip.widget.clone().upcast();
        let expander = find_expander(&widget).expect("the filter strip contains a rule expander");
        assert!(!expander.is_expanded());

        strip.update_filter(FilterSet {
            rules: vec![FilterRule {
                column: "name".into(),
                op: FilterOp::Eq,
                value: Some(FilterValue::Single("bookie".into())),
            }],
            ..FilterSet::default()
        });

        assert!(expander.is_expanded());
    }

    fn find_all<T: IsA<gtk::Widget>>(widget: &gtk::Widget, found: &mut Vec<T>) {
        if let Ok(typed) = widget.clone().downcast::<T>() {
            found.push(typed);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            find_all(&current, found);
            child = current.next_sibling();
        }
    }

    fn button_labelled(widget: &gtk::Widget, label: &str) -> gtk::Button {
        let mut buttons = Vec::new();
        find_all::<gtk::Button>(widget, &mut buttons);
        buttons
            .into_iter()
            .find(|button| button.label().as_deref() == Some(label))
            .unwrap_or_else(|| panic!("no button labelled {label}"))
    }

    #[test]
    #[ignore = "requires isolated GTK display"]
    fn the_where_field_apply_and_clear_share_one_row() {
        gtk::init().unwrap();
        let strip = build(Vec::new(), FilterSet::default(), Rc::new(|_| {}), None);
        let widget: gtk::Widget = strip.widget.clone().upcast();
        let mut entries = Vec::new();
        find_all::<gtk::Entry>(&widget, &mut entries);
        let raw = entries.into_iter().next().expect("the raw WHERE entry");
        let apply = button_labelled(&widget, "Apply");
        let clear = button_labelled(&widget, "Clear all");
        assert_eq!(apply.parent(), clear.parent());
        assert_eq!(
            raw.parent().and_then(|row| row.parent()),
            apply.parent().and_then(|actions| actions.parent())
        );
    }

    #[test]
    #[ignore = "requires isolated GTK display"]
    fn the_filter_strip_fits_a_narrow_window() {
        gtk::init().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let context = crate::ui::filter_presets::FilterPresetContext {
            store: crate::services::named_filter_presets::NamedFilterPresetsStore::load_at(
                directory.path().join("presets.json"),
            ),
            connection: uuid::Uuid::new_v4(),
            schema: None,
            table: "jobs".into(),
            driver_id: "postgres".into(),
        };
        let strip = build(Vec::new(), FilterSet::default(), Rc::new(|_| {}), Some(context));
        let (minimum, _, _, _) = strip.widget.measure(gtk::Orientation::Horizontal, -1);
        assert!(minimum <= 420, "the filter strip needs {minimum}px at minimum");
    }

    #[test]
    #[ignore = "requires isolated GTK display"]
    fn saved_filter_controls_live_in_one_menu_on_the_where_row() {
        gtk::init().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let context = crate::ui::filter_presets::FilterPresetContext {
            store: crate::services::named_filter_presets::NamedFilterPresetsStore::load_at(
                directory.path().join("presets.json"),
            ),
            connection: uuid::Uuid::new_v4(),
            schema: None,
            table: "jobs".into(),
            driver_id: "postgres".into(),
        };
        let strip = build(Vec::new(), FilterSet::default(), Rc::new(|_| {}), Some(context));
        let widget: gtk::Widget = strip.widget.clone().upcast();
        let mut menus = Vec::new();
        find_all::<gtk::MenuButton>(&widget, &mut menus);
        let saved = menus
            .into_iter()
            .find(|menu| menu.label().as_deref() == Some("Saved filters"))
            .expect("a Saved filters menu");
        let content = saved.popover().and_then(|popover| popover.child()).unwrap();
        for label in ["Apply saved", "Delete", "Save as", "Rename"] {
            button_labelled(&content, label);
        }
        let mut entries = Vec::new();
        find_all::<gtk::Entry>(&widget, &mut entries);
        let raw = entries
            .iter()
            .find(|entry| entry.placeholder_text().is_some_and(|text| text.starts_with("e.g.")))
            .unwrap();
        assert_eq!(
            saved.parent().and_then(|actions| actions.parent()),
            raw.parent().and_then(|row| row.parent())
        );
    }

    fn find_expander(widget: &gtk::Widget) -> Option<gtk::Expander> {
        if let Ok(expander) = widget.clone().downcast::<gtk::Expander>() {
            return Some(expander);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(expander) = find_expander(&current) {
                return Some(expander);
            }
            child = current.next_sibling();
        }
        None
    }
}
