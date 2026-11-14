use relm4::adw;
use relm4::adw::prelude::*;
use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender};
use relm4::gtk;
use relm4::gtk::gdk;
use relm4::gtk::glib;
use relm4::gtk::pango;
use std::cell::RefCell;
use std::rc::Rc;

use bookie_core::TableInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarObjectKind {
    Table,
    View,
    Group,
    Schema,
    Catalog,
}

#[derive(Debug, Clone)]
pub struct GroupInit {
    pub label: String,
    pub count: usize,
    pub collapsed: bool,
    pub actions: bool,
    pub entry: bool,
    pub new_schema: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SchemaCatalogActions {
    pub create: bool,
    pub drop: bool,
    pub hide: bool,
}

#[derive(Debug, Clone)]
pub struct SidebarRowInit {
    pub info: TableInfo,
    pub kind: SidebarObjectKind,
    pub depth: u8,
    pub group: Option<GroupInit>,
    pub column_source: Option<super::sidebar_columns::ColumnSource>,
    pub schema_actions: SchemaCatalogActions,
}

fn chevron_icon(collapsed: bool) -> &'static str {
    if collapsed {
        "pan-end-symbolic"
    } else {
        "pan-down-symbolic"
    }
}

fn sidebar_icon(kind: SidebarObjectKind) -> &'static str {
    match kind {
        SidebarObjectKind::Table => "view-list-symbolic",
        SidebarObjectKind::View => "view-paged-symbolic",
        SidebarObjectKind::Group => "folder-symbolic",
        SidebarObjectKind::Schema => "folder-symbolic",
        SidebarObjectKind::Catalog => "emblem-system-symbolic",
    }
}

fn context_menu_model(kind: SidebarObjectKind) -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();
    let open_section = gtk::gio::Menu::new();
    open_section.append(
        Some(&bookie_app_services::tr!("Open in new tab")),
        Some("sidebar-row.open-in-new-tab"),
    );
    menu.append_section(None, &open_section);
    if kind == SidebarObjectKind::Table {
        append_table_context_sections(&menu);
    }
    menu
}

fn append_table_context_sections(menu: &gtk::gio::Menu) {
    let structure_section = gtk::gio::Menu::new();
    structure_section.append(
        Some(&bookie_app_services::tr!("Edit Structure")),
        Some("sidebar-row.edit-structure"),
    );
    structure_section.append(
        Some(&bookie_app_services::tr!("Show CREATE TABLE")),
        Some("sidebar-row.show-create-table"),
    );
    structure_section.append(
        Some(&bookie_app_services::tr!("Join Referenced Tables")),
        Some("sidebar-row.join-referenced"),
    );
    structure_section.append(
        Some(&bookie_app_services::tr!("Show ER Diagram")),
        Some("sidebar-row.show-diagram"),
    );
    menu.append_section(None, &structure_section);
    let mutate_section = gtk::gio::Menu::new();
    mutate_section.append(
        Some(&bookie_app_services::tr!("Import CSV\u{2026}")),
        Some("sidebar-row.import-csv"),
    );
    mutate_section.append(
        Some(&bookie_app_services::tr!("Rename Table\u{2026}")),
        Some("sidebar-row.rename-table"),
    );
    mutate_section.append(
        Some(&bookie_app_services::tr!("Truncate Table\u{2026}")),
        Some("sidebar-row.truncate-table"),
    );
    mutate_section.append(
        Some(&bookie_app_services::tr!("Drop Table\u{2026}")),
        Some("sidebar-row.drop-table"),
    );
    menu.append_section(None, &mutate_section);
}

fn context_popover(
    state: &Rc<RefCell<Option<gtk::PopoverMenu>>>,
    root: &gtk::ListBoxRow,
    kind: SidebarObjectKind,
) -> gtk::PopoverMenu {
    if let Some(popover) = state.borrow().as_ref() {
        return popover.clone();
    }
    let popover = crate::ui::context_menu::from_model(Some(&context_menu_model(kind)));
    popover.set_parent(root);
    *state.borrow_mut() = Some(popover.clone());
    popover
}

#[derive(Debug)]
pub struct SidebarRow {
    pub info: TableInfo,
    kind: SidebarObjectKind,
    depth: u8,
    group: Option<GroupInit>,
    row: Option<gtk::ListBoxRow>,
    chevron: gtk::Image,
    new_table_button: Option<gtk::Button>,
    csv_button: Option<gtk::Button>,
    new_schema_button: Option<gtk::Button>,
    open_button: Option<gtk::Button>,
    schema_actions: SchemaCatalogActions,
    // Keep the parented popover so shutdown can unparent it before GTK finalizes the row.
    popover: Rc<RefCell<Option<gtk::PopoverMenu>>>,
    columns: super::sidebar_columns::SidebarColumns,
}

#[derive(Debug, Clone)]
pub enum SidebarRowMsg {
    ToggleColumns(bool),
    Open,
    OpenInNewTab,
    EditStructure,
    ShowCreateTable,
    JoinReferenced,
    ImportCsv,
    RenameTable,
    TruncateTable,
    DropTable,
    SetCollapsed(bool),
    NewTable,
    TableFromCsv,
    HideSchema,
    ShowDiagram,
    CompareSchema,
    NewSchema,
    DropSchema,
}

#[derive(Debug)]
pub enum SidebarRowOutput {
    ShowDiagram {
        schema: Option<String>,
    },
    CompareSchema {
        schema: Option<String>,
    },
    NewTable {
        schema: Option<String>,
    },
    TableFromCsv {
        schema: Option<String>,
    },
    Open {
        schema: Option<String>,
        name: String,
    },
    /// Ctrl+click and the context menu always append a tab, even when the table is already open.
    /// Plain click and Enter use the parent ListBox activation.
    OpenInNewTab {
        schema: Option<String>,
        name: String,
    },
    EditStructure {
        schema: Option<String>,
        name: String,
    },
    ShowCreateTable {
        schema: Option<String>,
        name: String,
    },
    JoinReferenced {
        schema: Option<String>,
        name: String,
    },
    ImportCsv {
        schema: Option<String>,
        name: String,
    },
    RenameTable {
        schema: Option<String>,
        name: String,
    },
    TruncateTable {
        schema: Option<String>,
        name: String,
    },
    DropTable {
        schema: Option<String>,
        name: String,
    },
    HideSchema(String),
    NewSchema,
    DropSchema {
        schema: String,
    },
}

fn group_button(icon: &str, tooltip: String) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(&tooltip)
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    button.update_property(&[gtk::accessible::Property::Label(&tooltip)]);
    button
}

#[relm4::factory(pub)]
impl FactoryComponent for SidebarRow {
    type Init = SidebarRowInit;
    type Input = SidebarRowMsg;
    type Output = SidebarRowOutput;
    type CommandOutput = super::sidebar_columns::ColumnReply;
    type ParentWidget = gtk::ListBox;

    view! {
        // Avoid AdwActionRow's settings-row height; the parent ListBox supplies sidebar styling.
        gtk::ListBoxRow {
            set_activatable: true,
            // The parent row-activated handler covers mouse and Enter; retain the name for its selection lookup.
            set_widget_name: &self.info.name,

            #[name = "row_content"]
            #[wrap(Some)]
            set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,

                #[name = "row_layout"]
                adw::BreakpointBin {
                    set_width_request: 180,
                    set_height_request: 36,
                    #[wrap(Some)]
                    set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_margin_start: 12 + 14 * i32::from(self.depth),
                set_margin_end: 12,
                set_margin_top: 6,
                set_margin_bottom: 6,

                        #[name = "title_row"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 8,
                            set_hexpand: true,

                            append: &self.chevron,

                            gtk::Image {
                                set_icon_name: Some(sidebar_icon(self.kind)),
                                set_pixel_size: 16,
                                set_visible: self.group.as_ref().is_none_or(|group| group.entry),
                            },

                            gtk::Label {
                                set_label: &self.title(),
                                set_xalign: 0.0,
                                set_hexpand: true,
                                set_ellipsize: pango::EllipsizeMode::End,
                                set_css_classes: if self.group.as_ref().is_some_and(|group| !group.entry) { &["caption-heading"] } else { &[] },
                            },

                            gtk::Label {
                                set_label: &self.group.as_ref().map(|group| group.count.to_string()).unwrap_or_default(),
                                set_visible: self.group.as_ref().is_some_and(|group| !group.entry),
                                set_css_classes: &["dim-label", "caption"],
                            },
                        },

                        #[name = "actions_row"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 4,
                            set_halign: gtk::Align::End,
                        },
                },
                    },
            },
        }
    }

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        let open_button = init.group.is_none().then(|| {
            let open_label = bookie_app_services::tr!("Open {name}").replace("{name}", &init.info.name);
            let button = gtk::Button::builder()
                .icon_name("go-next-symbolic")
                .tooltip_text(&open_label)
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            button.update_property(&[gtk::accessible::Property::Label(&open_label)]);
            button
        });
        let (new_table_button, csv_button) = if init.group.as_ref().is_some_and(|group| group.actions) {
            let labels = match init.info.schema.as_deref() {
                Some(schema) => (
                    bookie_app_services::tr!("New Table in {schema}\u{2026}").replace("{schema}", schema),
                    bookie_app_services::tr!("Table from CSV in {schema}\u{2026}").replace("{schema}", schema),
                ),
                None => (
                    bookie_app_services::tr!("New Table\u{2026}"),
                    bookie_app_services::tr!("Table from CSV\u{2026}"),
                ),
            };
            (
                Some(group_button("list-add-symbolic", labels.0)),
                Some(group_button("document-open-symbolic", labels.1)),
            )
        } else {
            (None, None)
        };
        let new_schema_button = init.group.as_ref().is_some_and(|group| group.new_schema).then(|| {
            group_button(
                "folder-new-symbolic",
                bookie_app_services::tr!("New Schema\u{2026}").to_string(),
            )
        });
        let collapsed = init.group.as_ref().is_some_and(|group| group.collapsed);
        let chevron = gtk::Image::from_icon_name(chevron_icon(collapsed));
        chevron.set_visible(init.group.as_ref().is_some_and(|group| !group.entry));
        Self {
            info: init.info,
            kind: init.kind,
            depth: init.depth,
            group: init.group,
            row: None,
            chevron,
            new_table_button,
            csv_button,
            new_schema_button,
            open_button,
            schema_actions: init.schema_actions,
            popover: Rc::new(RefCell::new(None)),
            columns: super::sidebar_columns::SidebarColumns::new(init.column_source),
        }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        // GTK requires unparenting a factory row's popover before finalizing the row.
        self.columns.close();
        if let Some(popover) = self.popover.borrow_mut().take() {
            popover.popdown();
            popover.unparent();
        }
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let widgets = view_output!();
        let narrow = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            260.0,
            adw::LengthUnit::Px,
        ));
        if let Some(row_layout) = widgets.row_layout.child().and_downcast::<gtk::Box>() {
            narrow.add_setter(&row_layout, "orientation", Some(&gtk::Orientation::Vertical.into()));
        }
        widgets.row_layout.add_breakpoint(narrow);
        if self.group.is_none() {
            self.columns.install(&root, &widgets.actions_row, &sender);
        }
        if let Some(button) = &self.new_table_button {
            widgets.actions_row.append(button);
        }
        if let Some(button) = &self.csv_button {
            widgets.actions_row.append(button);
        }
        if let Some(button) = &self.new_schema_button {
            widgets.actions_row.append(button);
        }
        if let Some(button) = &self.open_button {
            widgets.actions_row.append(button);
        }
        if self.group.is_some() {
            self.describe_group(&root);
            self.wire_group_buttons(&sender);
            if self.kind == SidebarObjectKind::Schema {
                self.wire_schema_menu(&root, &sender);
            }
            return widgets;
        }
        if let Some(button) = &self.open_button {
            let sender_for_open_button = sender.clone();
            button.connect_clicked(move |_| sender_for_open_button.input(SidebarRowMsg::Open));
        }

        // Qualified tooltips disambiguate schemas; plain-name tooltips keep single-schema accessibility consistent.
        let tooltip = match self.info.schema.as_deref().filter(|s| !s.is_empty()) {
            Some(schema) => format!("{schema}.{}", self.info.name),
            None => self.info.name.clone(),
        };
        root.set_tooltip_text(Some(&tooltip));

        // Claim Ctrl+click before row activation so the parent opens a new tab.
        let click_gesture = gtk::GestureClick::builder().button(1).build();
        let sender_for_ctrl = sender.clone();
        click_gesture.connect_pressed(move |gesture, _, _, _| {
            let state = gesture.current_event_state();
            if state.contains(gdk::ModifierType::CONTROL_MASK) {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                sender_for_ctrl.input(SidebarRowMsg::OpenInNewTab);
            }
        });
        root.add_controller(click_gesture);

        // The popover resolves actions through its row parent.
        let group = gtk::gio::SimpleActionGroup::new();
        let sender_open = sender.clone();
        let open_action = gtk::gio::ActionEntry::builder("open-in-new-tab")
            .activate(move |_, _, _| sender_open.input(SidebarRowMsg::OpenInNewTab))
            .build();
        let sender_edit = sender.clone();
        let edit_action = gtk::gio::ActionEntry::builder("edit-structure")
            .activate(move |_, _, _| sender_edit.input(SidebarRowMsg::EditStructure))
            .build();
        let sender_show = sender.clone();
        let show_create_action = gtk::gio::ActionEntry::builder("show-create-table")
            .activate(move |_, _, _| sender_show.input(SidebarRowMsg::ShowCreateTable))
            .build();
        let sender_join = sender.clone();
        let join_action = gtk::gio::ActionEntry::builder("join-referenced")
            .activate(move |_, _, _| sender_join.input(SidebarRowMsg::JoinReferenced))
            .build();
        let sender_diagram = sender.clone();
        let diagram_action = gtk::gio::ActionEntry::builder("show-diagram")
            .activate(move |_, _, _| sender_diagram.input(SidebarRowMsg::ShowDiagram))
            .build();
        let sender_drop = sender.clone();
        let sender_import = sender.clone();
        let import_action = gtk::gio::ActionEntry::builder("import-csv")
            .activate(move |_, _, _| sender_import.input(SidebarRowMsg::ImportCsv))
            .build();
        let sender_rename = sender.clone();
        let rename_action = gtk::gio::ActionEntry::builder("rename-table")
            .activate(move |_, _, _| sender_rename.input(SidebarRowMsg::RenameTable))
            .build();
        let sender_truncate = sender.clone();
        let truncate_action = gtk::gio::ActionEntry::builder("truncate-table")
            .activate(move |_, _, _| sender_truncate.input(SidebarRowMsg::TruncateTable))
            .build();
        let drop_action = gtk::gio::ActionEntry::builder("drop-table")
            .activate(move |_, _, _| sender_drop.input(SidebarRowMsg::DropTable))
            .build();
        group.add_action_entries([
            open_action,
            edit_action,
            show_create_action,
            join_action,
            diagram_action,
            import_action,
            rename_action,
            truncate_action,
            drop_action,
        ]);
        root.insert_action_group("sidebar-row", Some(&group));

        // Pop down before row removal so GTK cannot finalize an open popover.
        let popover_for_unmap = self.popover.clone();
        root.connect_unmap(move |_| {
            if let Some(popover) = popover_for_unmap.borrow().as_ref() {
                popover.popdown();
            }
        });

        let right_click = gtk::GestureClick::builder().button(3).build();
        let popover_for_right = self.popover.clone();
        let root_for_right = root.downgrade();
        let kind_for_right = self.kind;
        right_click.connect_pressed(move |g, _, x, y| {
            g.set_state(gtk::EventSequenceState::Claimed);
            let Some(root) = root_for_right.upgrade() else {
                return;
            };
            let popover = context_popover(&popover_for_right, &root, kind_for_right);
            crate::ui::context_menu::popup_at(&popover, Some(gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        });
        root.add_controller(right_click);

        // The Menu key anchors to the row center because no pointer position exists.
        let popover_for_menu = self.popover.clone();
        let root_for_menu = root.downgrade();
        let kind_for_menu = self.kind;
        let menu_shortcut = gtk::Shortcut::builder()
            .trigger(&crate::ui::shortcut::parse("Menu"))
            .action(&gtk::CallbackAction::new(move |_, _| {
                if let Some(root) = root_for_menu.upgrade() {
                    let popover = context_popover(&popover_for_menu, &root, kind_for_menu);
                    let pointing = root.compute_bounds(&root).map(|bounds| {
                        gdk::Rectangle::new((bounds.width() / 2.0) as i32, (bounds.height() / 2.0) as i32, 1, 1)
                    });
                    crate::ui::context_menu::popup_at(&popover, pointing);
                }
                glib::Propagation::Stop
            }))
            .build();
        let shortcut_controller = gtk::ShortcutController::new();
        shortcut_controller.add_shortcut(menu_shortcut);
        root.add_controller(shortcut_controller);

        widgets
    }

    fn update_cmd(&mut self, reply: Self::CommandOutput, _sender: FactorySender<Self>) {
        self.columns.apply(reply);
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        tracing::trace!(
            target: "bookie_app::sidebar_row",
            table = %self.info.name,
            ?msg,
            "input"
        );
        let (schema, name) = (self.info.schema.clone(), self.info.name.clone());
        let output = match msg {
            SidebarRowMsg::ToggleColumns(active) => return self.columns.toggle(active, &self.info, sender),
            SidebarRowMsg::SetCollapsed(collapsed) => return self.apply_collapse(collapsed),
            SidebarRowMsg::Open => SidebarRowOutput::Open { schema, name },
            SidebarRowMsg::OpenInNewTab => SidebarRowOutput::OpenInNewTab { schema, name },
            SidebarRowMsg::EditStructure => SidebarRowOutput::EditStructure { schema, name },
            SidebarRowMsg::ShowCreateTable => SidebarRowOutput::ShowCreateTable { schema, name },
            SidebarRowMsg::JoinReferenced => SidebarRowOutput::JoinReferenced { schema, name },
            SidebarRowMsg::ImportCsv => SidebarRowOutput::ImportCsv { schema, name },
            SidebarRowMsg::RenameTable => SidebarRowOutput::RenameTable { schema, name },
            SidebarRowMsg::TruncateTable => SidebarRowOutput::TruncateTable { schema, name },
            SidebarRowMsg::DropTable => SidebarRowOutput::DropTable { schema, name },
            SidebarRowMsg::NewTable => SidebarRowOutput::NewTable { schema },
            SidebarRowMsg::TableFromCsv => SidebarRowOutput::TableFromCsv { schema },
            SidebarRowMsg::HideSchema => SidebarRowOutput::HideSchema(schema.unwrap_or_default()),
            SidebarRowMsg::ShowDiagram => SidebarRowOutput::ShowDiagram { schema },
            SidebarRowMsg::CompareSchema => SidebarRowOutput::CompareSchema { schema },
            SidebarRowMsg::NewSchema => SidebarRowOutput::NewSchema,
            SidebarRowMsg::DropSchema => SidebarRowOutput::DropSchema {
                schema: schema.unwrap_or(name),
            },
        };
        let _ = sender.output(output);
    }
}

impl SidebarRow {
    fn apply_collapse(&self, collapsed: bool) {
        self.chevron.set_icon_name(Some(chevron_icon(collapsed)));
        if let Some(row) = &self.row {
            row.update_state(&[gtk::accessible::State::Expanded(Some(!collapsed))]);
        }
    }

    fn describe_group(&mut self, root: &gtk::ListBoxRow) {
        let Some(group) = &self.group else {
            return;
        };
        root.update_property(&[gtk::accessible::Property::Label(&group.label)]);
        if !group.entry {
            root.update_property(&[gtk::accessible::Property::Description(&group.count.to_string())]);
            root.update_state(&[gtk::accessible::State::Expanded(Some(!group.collapsed))]);
        }
        self.row = Some(root.clone());
    }

    fn title(&self) -> String {
        match &self.group {
            Some(group) => group.label.clone(),
            None => self.info.name.clone(),
        }
    }

    fn wire_group_buttons(&self, sender: &FactorySender<Self>) {
        if let Some(button) = &self.new_table_button {
            let new_table = sender.clone();
            button.connect_clicked(move |_| new_table.input(SidebarRowMsg::NewTable));
        }
        if let Some(button) = &self.csv_button {
            let from_csv = sender.clone();
            button.connect_clicked(move |_| from_csv.input(SidebarRowMsg::TableFromCsv));
        }
        if let Some(button) = &self.new_schema_button {
            let new_schema = sender.clone();
            button.connect_clicked(move |_| new_schema.input(SidebarRowMsg::NewSchema));
        }
    }

    fn wire_schema_menu(&mut self, root: &gtk::ListBoxRow, sender: &FactorySender<Self>) {
        let compare_enabled = std::env::var_os("BOOKIE_SCHEMA_COMPARE").is_some();
        let menu = schema_menu(compare_enabled, self.schema_actions);
        let popover = crate::ui::context_menu::from_model(Some(&menu));
        popover.set_parent(root);
        root.insert_action_group(
            "sidebar-row",
            Some(&schema_action_group(sender, self.schema_actions, compare_enabled)),
        );
        attach_schema_popover(root, &popover);
        *self.popover.borrow_mut() = Some(popover);
    }
}

fn schema_action(
    name: &str,
    sender: &FactorySender<SidebarRow>,
    msg: SidebarRowMsg,
) -> gtk::gio::ActionEntry<gtk::gio::SimpleActionGroup> {
    let sender = sender.clone();
    gtk::gio::ActionEntry::builder(name)
        .activate(move |_, _, _| sender.input(msg.clone()))
        .build()
}

fn schema_action_group(
    sender: &FactorySender<SidebarRow>,
    actions: SchemaCatalogActions,
    compare_enabled: bool,
) -> gtk::gio::SimpleActionGroup {
    let group = gtk::gio::SimpleActionGroup::new();
    group.add_action_entries([schema_action("show-diagram", sender, SidebarRowMsg::ShowDiagram)]);
    if actions.create {
        group.add_action_entries([schema_action("new-schema", sender, SidebarRowMsg::NewSchema)]);
    }
    if actions.drop {
        group.add_action_entries([schema_action("drop-schema", sender, SidebarRowMsg::DropSchema)]);
    }
    if compare_enabled {
        group.add_action_entries([schema_action("compare-schema", sender, SidebarRowMsg::CompareSchema)]);
    }
    if actions.hide {
        group.add_action_entries([schema_action("hide-schema", sender, SidebarRowMsg::HideSchema)]);
    }
    group
}

fn attach_schema_popover(root: &gtk::ListBoxRow, popover: &gtk::PopoverMenu) {
    let popover_for_unmap = popover.clone();
    root.connect_unmap(move |_| popover_for_unmap.popdown());
    let right_click = gtk::GestureClick::builder().button(3).build();
    let popover_for_right = popover.clone();
    right_click.connect_pressed(move |gesture, _, x, y| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        crate::ui::context_menu::popup_at(&popover_for_right, Some(gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    });
    root.add_controller(right_click);
    let popover_for_shortcut = popover.clone();
    let root_for_shortcut = root.downgrade();
    let menu_shortcut = gtk::Shortcut::builder()
        .trigger(&crate::ui::shortcut::parse("Menu"))
        .action(&gtk::CallbackAction::new(move |_, _| {
            let pointing = root_for_shortcut.upgrade().and_then(|root| {
                root.compute_bounds(&root).map(|bounds| {
                    gdk::Rectangle::new((bounds.width() / 2.0) as i32, (bounds.height() / 2.0) as i32, 1, 1)
                })
            });
            crate::ui::context_menu::popup_at(&popover_for_shortcut, pointing);
            glib::Propagation::Stop
        }))
        .build();
    let shortcut_controller = gtk::ShortcutController::new();
    shortcut_controller.add_shortcut(menu_shortcut);
    root.add_controller(shortcut_controller);
}

fn schema_menu(compare_enabled: bool, actions: SchemaCatalogActions) -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();
    menu.append(
        Some(&bookie_app_services::tr!("Show ER Diagram")),
        Some("sidebar-row.show-diagram"),
    );
    if actions.create {
        menu.append(
            Some(&bookie_app_services::tr!("New Schema\u{2026}")),
            Some("sidebar-row.new-schema"),
        );
    }
    if actions.drop {
        menu.append(
            Some(&bookie_app_services::tr!("Drop Schema\u{2026}")),
            Some("sidebar-row.drop-schema"),
        );
    }
    if compare_enabled {
        menu.append(
            Some(&bookie_app_services::tr!("Compare with another schema…")),
            Some("sidebar-row.compare-schema"),
        );
    }
    if actions.hide {
        menu.append(
            Some(&bookie_app_services::tr!("Hide schema")),
            Some("sidebar-row.hide-schema"),
        );
    }
    menu
}

#[cfg(test)]
mod tests {
    use super::*;
    use relm4::factory::FactoryVecDeque;

    #[test]
    fn schema_compare_menu_requires_explicit_opt_in() {
        use gtk::gio::prelude::MenuModelExt;

        let catalog = SchemaCatalogActions {
            create: true,
            drop: true,
            hide: true,
        };
        for enabled in [false, true] {
            let menu = schema_menu(enabled, catalog);
            let actions: Vec<_> = (0..menu.n_items())
                .filter_map(|index| menu.item_attribute_value(index, "action", None))
                .filter_map(|value| value.get::<String>())
                .collect();
            assert_eq!(actions.contains(&"sidebar-row.compare-schema".to_owned()), enabled);
            assert!(actions.contains(&"sidebar-row.show-diagram".to_owned()));
            assert!(actions.contains(&"sidebar-row.hide-schema".to_owned()));
            assert!(actions.contains(&"sidebar-row.new-schema".to_owned()));
            assert!(actions.contains(&"sidebar-row.drop-schema".to_owned()));
        }
    }

    #[test]
    fn schema_catalog_actions_omit_unsupported_entries() {
        use gtk::gio::prelude::MenuModelExt;

        let menu = schema_menu(false, SchemaCatalogActions::default());
        let actions: Vec<_> = (0..menu.n_items())
            .filter_map(|index| menu.item_attribute_value(index, "action", None))
            .filter_map(|value| value.get::<String>())
            .collect();
        assert_eq!(actions, vec!["sidebar-row.show-diagram".to_owned()]);
    }

    fn pump() {
        let context = glib::MainContext::default();
        while context.iteration(false) {}
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn a_group_row_shows_a_chevron_that_follows_its_collapse_state() {
        relm4::adw::init().unwrap();
        let list = gtk::ListBox::new();
        let mut factory = FactoryVecDeque::<SidebarRow>::builder().launch(list.clone()).detach();
        factory.guard().push_back(SidebarRowInit {
            info: TableInfo {
                schema: None,
                name: "schema:/tables".into(),
            },
            kind: SidebarObjectKind::Group,
            depth: 0,
            column_source: None,
            group: Some(GroupInit {
                label: "Tables".into(),
                count: 3,
                collapsed: false,
                actions: true,
                entry: false,
                new_schema: false,
            }),
            schema_actions: SchemaCatalogActions::default(),
        });
        factory.guard().push_back(SidebarRowInit {
            info: TableInfo {
                schema: None,
                name: "users".into(),
            },
            kind: SidebarObjectKind::Table,
            depth: 1,
            group: None,
            column_source: None,
            schema_actions: SchemaCatalogActions::default(),
        });
        pump();
        let chevron_of = |index: i32| {
            let chevron = &factory.get(index as usize).unwrap().chevron;
            (chevron.icon_name().map(|name| name.to_string()), chevron.is_visible())
        };
        assert_eq!(chevron_of(0), (Some("pan-down-symbolic".into()), true));
        assert!(!chevron_of(1).1, "an object row has no chevron");

        factory.send(0, SidebarRowMsg::SetCollapsed(true));
        pump();

        assert_eq!(chevron_of(0).0.as_deref(), Some("pan-end-symbolic"));
    }
}

#[cfg(test)]
mod catalog_entry_tests {
    use super::*;
    use relm4::factory::FactoryVecDeque;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn a_catalog_entry_is_a_leaf_row_without_a_chevron_and_with_an_icon() {
        relm4::adw::init().unwrap();
        let list = gtk::ListBox::new();
        let mut factory = FactoryVecDeque::<SidebarRow>::builder().launch(list.clone()).detach();
        factory.guard().push_back(SidebarRowInit {
            info: TableInfo {
                schema: None,
                name: "catalog/role".into(),
            },
            kind: SidebarObjectKind::Catalog,
            depth: 1,
            column_source: None,
            group: Some(GroupInit {
                label: "Roles".into(),
                count: 0,
                collapsed: false,
                actions: false,
                entry: true,
                new_schema: false,
            }),
            schema_actions: SchemaCatalogActions::default(),
        });
        let context = glib::MainContext::default();
        while context.iteration(false) {}
        let row = factory.get(0).unwrap();
        assert!(!row.chevron.is_visible());
        assert_eq!(sidebar_icon(row.kind), "emblem-system-symbolic");
    }
}

#[cfg(test)]
mod responsive_layout_tests {
    use super::*;
    use relm4::factory::FactoryVecDeque;

    fn row_at_width(width: i32) -> (gtk::Window, FactoryVecDeque<SidebarRow>, gtk::ListBoxRow) {
        relm4::adw::init().unwrap();
        let list = gtk::ListBox::new();
        let mut factory = FactoryVecDeque::<SidebarRow>::builder().launch(list.clone()).detach();
        factory.guard().push_back(SidebarRowInit {
            info: TableInfo {
                schema: Some("public".into()),
                name: "a_very_long_table_name_for_narrow_sidebar_testing".into(),
            },
            kind: SidebarObjectKind::Table,
            depth: 1,
            group: None,
            column_source: None,
            schema_actions: SchemaCatalogActions::default(),
        });
        let window = gtk::Window::builder()
            .default_width(width)
            .default_height(180)
            .child(&list)
            .build();
        window.present();
        let context = glib::MainContext::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while factory.get(0).is_none_or(|_| list.row_at_index(0).is_none()) && std::time::Instant::now() < deadline {
            context.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        while context.iteration(false) {}
        (window, factory, list.row_at_index(0).unwrap())
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn row_actions_reflow_at_supported_sidebar_widths_with_names_and_focus_order() {
        for (width, orientation) in [(220, gtk::Orientation::Vertical), (280, gtk::Orientation::Horizontal)] {
            let (window, _factory, row) = row_at_width(width);
            let body = row.child().and_downcast::<gtk::Box>().unwrap();
            let breakpoint = body.first_child().and_downcast::<relm4::adw::BreakpointBin>().unwrap();
            let row_content = breakpoint.child().and_downcast::<gtk::Box>().unwrap();
            assert_eq!(
                row_content.orientation(),
                orientation,
                "requested={width}, window={}, row={}, breakpoint={}, active={}",
                window.width(),
                row.width(),
                breakpoint.width(),
                breakpoint.current_breakpoint().is_some()
            );
            let title = row_content.first_child().and_downcast::<gtk::Box>().unwrap();
            let actions = title.next_sibling().and_downcast::<gtk::Box>().unwrap();
            let title_y = title.compute_bounds(&row_content).unwrap().y();
            let actions_y = actions.compute_bounds(&row_content).unwrap().y();
            assert_eq!(
                actions_y > title_y,
                width <= 260,
                "requested={width}, title_y={title_y}, actions_y={actions_y}"
            );
            let columns = actions.first_child().and_downcast::<gtk::ToggleButton>().unwrap();
            let open = columns.next_sibling().and_downcast::<gtk::Button>().unwrap();
            assert!(columns.is_focusable());
            assert!(open.is_focusable());
            assert!(columns.grab_focus());
            assert!(actions.child_focus(gtk::DirectionType::TabForward));
            assert_eq!(gtk::prelude::GtkWindowExt::focus(&window), Some(open.clone().upcast()));
            assert_eq!(
                open.tooltip_text().as_deref(),
                Some("Open a_very_long_table_name_for_narrow_sidebar_testing")
            );
            assert!(gtk::test_accessible_has_property(&open, gtk::AccessibleProperty::Label));
        }
    }
}

#[cfg(test)]
mod columns_affordance_tests {
    use super::*;
    use relm4::factory::FactoryVecDeque;

    fn action_button_count(widget: &gtk::Widget) -> usize {
        let own = usize::from(widget.is::<gtk::Button>() && !widget.is::<gtk::ToggleButton>());
        let mut count = own;
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            count += action_button_count(&current);
        }
        count
    }

    fn expansion_button(widget: &gtk::Widget) -> Option<gtk::ToggleButton> {
        if let Some(button) = widget.downcast_ref::<gtk::ToggleButton>() {
            return Some(button.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            if let Some(button) = expansion_button(&current) {
                return Some(button);
            }
        }
        None
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn a_table_has_a_separate_keyboard_reachable_columns_expander() {
        relm4::adw::init().unwrap();
        let list = gtk::ListBox::new();
        let mut factory = FactoryVecDeque::<SidebarRow>::builder().launch(list.clone()).detach();
        factory.guard().push_back(SidebarRowInit {
            info: TableInfo {
                schema: Some("public".into()),
                name: "items".into(),
            },
            kind: SidebarObjectKind::Table,
            depth: 1,
            group: None,
            column_source: None,
            schema_actions: SchemaCatalogActions::default(),
        });
        while glib::MainContext::default().iteration(false) {}
        let row = list.row_at_index(0).unwrap();
        assert_eq!(action_button_count(row.upcast_ref()), 1);
        let expander =
            expansion_button(row.upcast_ref()).expect("table columns can be expanded without opening the table");
        assert!(expander.is_focusable());
        assert!(!expander.is_active());
        let body = row.child().and_downcast::<gtk::Box>().unwrap();
        let reveal = body.last_child().and_downcast::<gtk::Revealer>().unwrap();
        assert!(reveal.child().is_none());
        expander.set_active(true);
        while glib::MainContext::default().iteration(false) {}
        assert!(reveal.child().is_some());
        assert!(reveal.reveals_child());
        expander.set_active(false);
        while glib::MainContext::default().iteration(false) {}
        assert!(!reveal.reveals_child());

        factory.guard().push_back(SidebarRowInit {
            info: TableInfo {
                schema: Some("public".into()),
                name: "public".into(),
            },
            kind: SidebarObjectKind::Schema,
            depth: 0,
            group: Some(GroupInit {
                label: "public".into(),
                count: 1,
                collapsed: false,
                actions: true,
                entry: false,
                new_schema: false,
            }),
            column_source: None,
            schema_actions: SchemaCatalogActions::default(),
        });
        factory.guard().push_back(SidebarRowInit {
            info: TableInfo {
                schema: None,
                name: "Roles".into(),
            },
            kind: SidebarObjectKind::Catalog,
            depth: 0,
            group: Some(GroupInit {
                label: "Roles".into(),
                count: 0,
                collapsed: false,
                actions: false,
                entry: false,
                new_schema: false,
            }),
            column_source: None,
            schema_actions: SchemaCatalogActions::default(),
        });
        while glib::MainContext::default().iteration(false) {}
        assert_eq!(action_button_count(list.row_at_index(1).unwrap().upcast_ref()), 2);
        assert_eq!(action_button_count(list.row_at_index(2).unwrap().upcast_ref()), 0);
    }
}

#[cfg(test)]
mod lazy_context_menu_tests {
    use super::*;
    use relm4::factory::FactoryVecDeque;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn a_table_context_menu_is_created_on_demand_and_resolves_row_actions() {
        relm4::adw::init().unwrap();
        let list = gtk::ListBox::new();
        let mut factory = FactoryVecDeque::<SidebarRow>::builder().launch(list.clone()).detach();
        factory.guard().push_back(SidebarRowInit {
            info: TableInfo {
                schema: Some("public".into()),
                name: "items".into(),
            },
            kind: SidebarObjectKind::Table,
            depth: 1,
            group: None,
            column_source: None,
            schema_actions: SchemaCatalogActions::default(),
        });
        while glib::MainContext::default().iteration(false) {}
        assert!(factory.get(0).is_some_and(|row| row.popover.borrow().is_none()));
        let row_widget = list.row_at_index(0).unwrap();
        let row_widget_weak = row_widget.downgrade();
        let row = factory.get(0).unwrap();
        let popover = context_popover(&row.popover, &row_widget, row.kind);
        assert!(popover.activate_action("sidebar-row.edit-structure", None).is_ok());
        assert!(popover.activate_action("sidebar-row.join-referenced", None).is_ok());
        assert!(popover.activate_action("sidebar-row.drop-table", None).is_ok());
        factory.guard().clear();
        while glib::MainContext::default().iteration(false) {}
        drop(popover);
        drop(row_widget);
        assert!(row_widget_weak.upgrade().is_none());
    }
}
