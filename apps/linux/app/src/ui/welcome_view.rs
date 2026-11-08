use relm4::adw::prelude::*;
use relm4::factory::FactoryVecDeque;
use relm4::prelude::*;
use relm4::{adw, gtk};

use std::collections::HashSet;
use std::sync::Arc;

use bookie_core::DriverRegistry;
use bookie_storage::{
    ConnectionOrganizationIndex, GroupTreeNode, SavedConnection, arrange_connections, connection_group_tree,
};
use uuid::Uuid;

use super::connection_row::{ConnectionRow, ConnectionRowInit, ConnectionRowOutput};

pub struct WelcomeView {
    registry: Arc<DriverRegistry>,
    connections: Vec<SavedConnection>,
    organization: ConnectionOrganizationIndex,
    filter: String,
    collapsed_groups: HashSet<String>,
    sections: Vec<FactoryVecDeque<ConnectionRow>>,
    sections_box: gtk::Box,
    stack: gtk::Stack,
    search: gtk::SearchEntry,
    empty_filter_page: adw::StatusPage,
    recent_group: adw::PreferencesGroup,
    recent_items: gtk::FlowBox,
}

#[derive(Debug)]
pub enum WelcomeViewInput {
    SetConnections(Vec<SavedConnection>),
    SetOrganization(ConnectionOrganizationIndex),
    FilterChanged(String),
    ToggleGroupCollapse(String),
    OpenConnect,
    ImportUrl,
    OpenSaved(Box<SavedConnection>),
    ToggleFavorite(Uuid),
    Organize(Box<SavedConnection>),
    Edit(Box<SavedConnection>),
    Duplicate(Uuid),
    Delete(Uuid),
    ExportBundle,
    ImportBundle,
}

#[derive(Debug)]
pub enum WelcomeViewOutput {
    OpenConnect,
    ImportUrl,
    OpenSaved(Box<SavedConnection>),
    ToggleFavorite(Uuid),
    Organize(Box<SavedConnection>),
    Edit(Box<SavedConnection>),
    Duplicate(Uuid),
    Delete(Uuid),
    ExportBundle,
    ImportBundle,
}

pub struct WelcomeViewInit {
    pub registry: Arc<DriverRegistry>,
}

impl SimpleComponent for WelcomeView {
    type Init = WelcomeViewInit;
    type Input = WelcomeViewInput;
    type Output = WelcomeViewOutput;
    type Root = gtk::Stack;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Stack::builder().build()
    }

    fn init(init: Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let empty_page = empty_page(&sender);
        root.add_named(&empty_page, Some("empty"));

        // Populated page — saved connections list. AdwClamp is the
        // GNOME pattern for "constrain reading width to a sensible
        // max in a scrollable area"; it centres + caps width without
        // the manual `gtk::Box` halign/margin gymnastics.
        let scroller = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let clamp = adw::Clamp::builder().maximum_size(560).build();
        let outer = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let heading = gtk::Label::builder()
            .label(crate::tr!("Welcome to BookiE"))
            .xalign(0.0)
            .wrap(true)
            .css_classes(["title-1"])
            .build();
        let introduction = gtk::Label::builder()
            .label(crate::tr!(
                "Choose a saved connection or add a database to get started."
            ))
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        outer.append(&heading);
        outer.append(&introduction);
        let recent_items = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .max_children_per_line(3)
            .homogeneous(true)
            .row_spacing(8)
            .column_spacing(8)
            .build();
        let recent_group = adw::PreferencesGroup::builder()
            .title(crate::tr!("Recently opened"))
            .visible(false)
            .build();
        recent_group.add(&recent_items);
        outer.append(&recent_group);

        // Single CTA on the populated page: the "+" button in the
        // group header. Previously we also rendered a bottom pill
        // labelled "New connection", which duplicated the affordance —
        // ambiguity at different visual weights. Empty-page pill
        // stays (it's the only CTA there); on this page the header
        // suffix is sufficient.
        let group = adw::PreferencesGroup::builder()
            .title(crate::tr!("Saved connections"))
            .build();
        let header_btn = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text(crate::tr!("Add Connection"))
            .valign(gtk::Align::Center)
            .build();
        header_btn.add_css_class("flat");
        let s_header = sender.clone();
        header_btn.connect_clicked(move |_| s_header.input(WelcomeViewInput::OpenConnect));
        // Import sits beside Add rather than inside a menu: pasting a
        // connection URL is the fastest path in from another tool, and
        // GNOME puts sibling create-actions side by side in the group
        // header (Software's "Add" / "Install File…" pattern).
        let import_btn = gtk::Button::builder()
            .icon_name("insert-link-symbolic")
            .tooltip_text(crate::tr!("Import from URL"))
            .valign(gtk::Align::Center)
            .build();
        import_btn.add_css_class("flat");
        let s_import = sender.clone();
        import_btn.connect_clicked(move |_| s_import.input(WelcomeViewInput::ImportUrl));
        let header_actions = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        let bundle_btn = gtk::MenuButton::builder()
            .icon_name("document-send-symbolic")
            .tooltip_text(crate::tr!("Export or import connections"))
            .valign(gtk::Align::Center)
            .build();
        bundle_btn.add_css_class("flat");
        let bundle_menu = gtk::gio::Menu::new();
        bundle_menu.append(Some(&crate::tr!("Export connections…")), Some("welcome.export-bundle"));
        bundle_menu.append(Some(&crate::tr!("Import connections…")), Some("welcome.import-bundle"));
        bundle_btn.set_menu_model(Some(&bundle_menu));

        let actions = gtk::gio::SimpleActionGroup::new();
        let export_action = gtk::gio::SimpleAction::new("export-bundle", None);
        let s_export = sender.clone();
        export_action.connect_activate(move |_, _| s_export.input(WelcomeViewInput::ExportBundle));
        actions.add_action(&export_action);
        let import_action = gtk::gio::SimpleAction::new("import-bundle", None);
        let s_bundle_import = sender.clone();
        import_action.connect_activate(move |_, _| s_bundle_import.input(WelcomeViewInput::ImportBundle));
        actions.add_action(&import_action);
        root.insert_action_group("welcome", Some(&actions));

        header_actions.append(&import_btn);
        header_actions.append(&bundle_btn);
        header_actions.append(&header_btn);
        group.set_header_suffix(Some(&header_actions));

        // Filter box above the list. Hidden until there are enough
        // connections to be worth filtering — a search field over three
        // rows is noise, and GNOME only reveals search once a list can
        // outgrow the viewport.
        let search = gtk::SearchEntry::builder()
            .placeholder_text(crate::tr!("Filter by name, group, tag or driver"))
            .hexpand(true)
            .visible(false)
            .build();
        let s_search = sender.clone();
        search.connect_search_changed(move |entry| {
            s_search.input(WelcomeViewInput::FilterChanged(entry.text().to_string()));
        });
        outer.append(&search);
        outer.append(&group);
        let sections_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .build();
        outer.append(&sections_box);

        // Shown in place of the list when every connection is filtered
        // out. Distinct from the "no connections yet" page: the fix is
        // to clear the filter, not to add a connection.
        let empty_filter_page = adw::StatusPage::builder()
            .icon_name("system-search-symbolic")
            .title(crate::tr!("No matches"))
            .description(crate::tr!("No saved connection matches this filter."))
            .visible(false)
            .build();
        empty_filter_page.add_css_class("compact");
        outer.append(&empty_filter_page);
        clamp.set_child(Some(&outer));

        scroller.set_child(Some(&clamp));
        root.add_named(&scroller, Some("populated"));
        root.set_visible_child_name("empty");

        let model = WelcomeView {
            registry: init.registry,
            connections: Vec::new(),
            organization: ConnectionOrganizationIndex::default(),
            filter: String::new(),
            collapsed_groups: HashSet::new(),
            sections: Vec::new(),
            sections_box,
            stack: root.clone(),
            search,
            empty_filter_page,
            recent_group,
            recent_items,
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            WelcomeViewInput::SetConnections(connections) => {
                self.connections = connections;
                self.rebuild_rows(&sender);
            }
            WelcomeViewInput::SetOrganization(organization) => {
                self.organization = organization;
                self.rebuild_rows(&sender);
            }
            WelcomeViewInput::FilterChanged(filter) => {
                self.filter = filter;
                self.rebuild_rows(&sender);
            }
            WelcomeViewInput::ToggleGroupCollapse(path) => {
                if !self.collapsed_groups.remove(&path) {
                    self.collapsed_groups.insert(path);
                }
                self.rebuild_rows(&sender);
            }
            WelcomeViewInput::OpenConnect => {
                let _ = sender.output(WelcomeViewOutput::OpenConnect);
            }
            WelcomeViewInput::ImportUrl => {
                let _ = sender.output(WelcomeViewOutput::ImportUrl);
            }
            WelcomeViewInput::OpenSaved(saved) => {
                let _ = sender.output(WelcomeViewOutput::OpenSaved(saved));
            }
            WelcomeViewInput::ToggleFavorite(id) => {
                let _ = sender.output(WelcomeViewOutput::ToggleFavorite(id));
            }
            WelcomeViewInput::Organize(saved) => {
                let _ = sender.output(WelcomeViewOutput::Organize(saved));
            }
            WelcomeViewInput::Edit(saved) => {
                let _ = sender.output(WelcomeViewOutput::Edit(saved));
            }
            WelcomeViewInput::Duplicate(id) => {
                let _ = sender.output(WelcomeViewOutput::Duplicate(id));
            }
            WelcomeViewInput::ExportBundle => {
                let _ = sender.output(WelcomeViewOutput::ExportBundle);
            }
            WelcomeViewInput::ImportBundle => {
                let _ = sender.output(WelcomeViewOutput::ImportBundle);
            }
            WelcomeViewInput::Delete(id) => {
                let _ = sender.output(WelcomeViewOutput::Delete(id));
            }
        }
    }
}

/// Connections a filter cannot outgrow. Below this the filter box stays
/// hidden, so the field only appears once scanning the list by eye stops
/// being the faster option.
const FILTER_REVEAL_THRESHOLD: usize = 6;

fn recent_connections(connections: &[SavedConnection]) -> Vec<&SavedConnection> {
    let mut recent: Vec<_> = connections
        .iter()
        .filter(|connection| connection.last_opened_at.is_some())
        .collect();
    recent.sort_by(|left, right| {
        right
            .last_opened_at
            .cmp(&left.last_opened_at)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
    recent.truncate(3);
    recent
}

fn recent_connection_detail(driver: &str, environment: bookie_core::Environment) -> String {
    let label = if environment == bookie_core::Environment::Prod {
        crate::tr!("Production")
    } else {
        environment.display_name().to_owned()
    };
    format!("{driver} · {label}")
}

impl WelcomeView {
    fn rebuild_rows(&mut self, sender: &ComponentSender<Self>) {
        self.rebuild_recent(sender);
        let arranged = arrange_connections(&self.connections, &self.organization, &self.filter);
        let tree = connection_group_tree(&arranged, &self.organization);

        self.sections.clear();
        while let Some(child) = self.sections_box.first_child() {
            self.sections_box.remove(&child);
        }
        for node in &tree {
            self.append_tree_node(node, 0, sender);
        }

        self.search
            .set_visible(self.connections.len() >= FILTER_REVEAL_THRESHOLD);
        self.empty_filter_page
            .set_visible(arranged.is_empty() && !self.connections.is_empty());
        let name = if self.connections.is_empty() {
            "empty"
        } else {
            "populated"
        };
        self.stack.set_visible_child_name(name);
    }

    fn append_tree_node(&mut self, node: &GroupTreeNode, depth: u32, sender: &ComponentSender<Self>) {
        let collapsed = node
            .path
            .as_ref()
            .is_some_and(|path| self.collapsed_groups.contains(path));
        let title = node.path.as_ref().map(|_| node.title.as_str());
        let section = section_group(title, depth);
        if let Some(path) = &node.path {
            let toggle = collapse_button(!collapsed);
            let path = path.clone();
            let sender = sender.clone();
            toggle.connect_clicked(move |_| sender.input(WelcomeViewInput::ToggleGroupCollapse(path.clone())));
            section.set_header_suffix(Some(&toggle));
        }
        self.sections_box.append(&section);
        if collapsed {
            return;
        }
        if !node.connections.is_empty() {
            let factory = self.build_connection_factory(&node.connections, sender);
            section.add(factory.widget());
            self.sections.push(factory);
        }
        for child in &node.children {
            self.append_tree_node(child, depth.saturating_add(1), sender);
        }
    }

    fn rebuild_recent(&self, sender: &ComponentSender<Self>) {
        while let Some(child) = self.recent_items.first_child() {
            self.recent_items.remove(&child);
        }
        let recent = recent_connections(&self.connections);
        self.recent_group
            .set_visible(!recent.is_empty() && self.filter.trim().is_empty());
        for saved in recent {
            let driver = self
                .registry
                .get(&saved.driver_id)
                .map_or_else(|| saved.driver_id.clone(), |driver| driver.display_name().to_string());
            let name = gtk::Label::builder()
                .label(&saved.name)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let detail = gtk::Label::builder()
                .label(recent_connection_detail(&driver, saved.environment))
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .css_classes(["dim-label", "caption"])
                .build();
            let content = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(4)
                .margin_top(8)
                .margin_bottom(8)
                .margin_start(8)
                .margin_end(8)
                .build();
            content.append(&name);
            content.append(&detail);
            let button = gtk::Button::builder().child(&content).hexpand(true).build();
            button.add_css_class("card");
            let label = crate::tr!("Open recent connection: {name}, {details}")
                .replace("{name}", &saved.name)
                .replace("{details}", &recent_connection_detail(&driver, saved.environment));
            button.update_property(&[gtk::accessible::Property::Label(&label)]);
            let saved = saved.clone();
            let sender = sender.clone();
            button.connect_clicked(move |_| sender.input(WelcomeViewInput::OpenSaved(Box::new(saved.clone()))));
            self.recent_items.insert(&button, -1);
        }
    }

    fn build_connection_factory(
        &self,
        members: &[SavedConnection],
        sender: &ComponentSender<Self>,
    ) -> FactoryVecDeque<ConnectionRow> {
        let mut factory: FactoryVecDeque<ConnectionRow> = FactoryVecDeque::builder()
            .launch(
                gtk::ListBox::builder()
                    .selection_mode(gtk::SelectionMode::None)
                    .css_classes(["boxed-list"])
                    .build(),
            )
            .forward(sender.input_sender(), |out| match out {
                ConnectionRowOutput::Open(saved) => WelcomeViewInput::OpenSaved(saved),
                ConnectionRowOutput::ToggleFavorite(id) => WelcomeViewInput::ToggleFavorite(id),
                ConnectionRowOutput::Organize(saved) => WelcomeViewInput::Organize(saved),
                ConnectionRowOutput::Edit(saved) => WelcomeViewInput::Edit(saved),
                ConnectionRowOutput::Duplicate(id) => WelcomeViewInput::Duplicate(id),
                ConnectionRowOutput::Delete(id) => WelcomeViewInput::Delete(id),
            });
        let mut guard = factory.guard();
        for saved in members {
            let organization = self.organization.get(saved.id);
            guard.push_back(ConnectionRowInit::new(saved.clone(), organization, &self.registry));
        }
        drop(guard);
        factory
    }
}

fn visible_group_paths(nodes: &[GroupTreeNode], collapsed: &HashSet<String>) -> Vec<Option<String>> {
    let mut visible = Vec::new();
    collect_visible_group_paths(nodes, collapsed, &mut visible);
    visible
}

fn collect_visible_group_paths(
    nodes: &[GroupTreeNode],
    collapsed: &HashSet<String>,
    visible: &mut Vec<Option<String>>,
) {
    for node in nodes {
        visible.push(node.path.clone());
        if node.path.as_ref().is_some_and(|path| collapsed.contains(path)) {
            continue;
        }
        collect_visible_group_paths(&node.children, collapsed, visible);
    }
}

fn collapse_button(expanded: bool) -> gtk::Button {
    let icon = if expanded {
        "pan-down-symbolic"
    } else {
        "pan-end-symbolic"
    };
    let label = if expanded {
        crate::tr!("Collapse group")
    } else {
        crate::tr!("Expand group")
    };
    gtk::Button::builder()
        .icon_name(icon)
        .has_frame(false)
        .tooltip_text(&label)
        .build()
}

fn empty_page(sender: &ComponentSender<WelcomeView>) -> adw::StatusPage {
    let page = adw::StatusPage::builder()
        .icon_name("network-server-symbolic")
        .title(crate::tr!("No connections yet"))
        .description(crate::tr!("Add a database connection to get started."))
        .build();
    let add = gtk::Button::builder()
        .label(crate::tr!("Add Connection"))
        .halign(gtk::Align::Center)
        .build();
    add.add_css_class("suggested-action");
    add.add_css_class("pill");
    let s_add = sender.clone();
    add.connect_clicked(move |_| s_add.input(WelcomeViewInput::OpenConnect));
    let import_bundle = gtk::Button::builder().label(crate::tr!("Import connections…")).build();
    let s_bundle = sender.clone();
    import_bundle.connect_clicked(move |_| s_bundle.input(WelcomeViewInput::ImportBundle));
    let import_url = gtk::Button::builder().label(crate::tr!("Import from URL…")).build();
    let s_url = sender.clone();
    import_url.connect_clicked(move |_| s_url.input(WelcomeViewInput::ImportUrl));
    let imports = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .halign(gtk::Align::Center)
        .build();
    for button in [&import_bundle, &import_url] {
        button.add_css_class("flat");
        imports.append(button);
    }
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .build();
    content.append(&add);
    content.append(&imports);
    page.set_child(Some(&content));
    page
}

pub(crate) fn section_group(group: Option<&str>, depth: u32) -> adw::PreferencesGroup {
    let title = group.map_or_else(|| crate::tr!("Ungrouped"), str::to_owned);
    let section = adw::PreferencesGroup::builder()
        .title(crate::ui::plain_text::escape_markup(&title))
        .build();
    if depth > 0 {
        section.set_margin_start(i32::try_from(depth.saturating_mul(12)).unwrap_or(96));
    }
    section
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::{AuthMode, Environment, TlsMode};
    use bookie_storage::ConnectionOrganization;

    fn saved(name: &str) -> SavedConnection {
        SavedConnection {
            auth_source: None,
            id: Uuid::new_v4(),
            name: name.into(),
            driver_id: "postgres".into(),
            host: "localhost".into(),
            port: 5432,
            socket_dir: None,
            socket_path: None,
            database: "app".into(),
            username: "app".into(),
            use_tls: false,
            tls_mode: Some(TlsMode::Disabled),
            tls_root_cert: None,
            tls_client_cert: None,
            tls_client_key: None,
            read_only: false,
            auth_mode: AuthMode::Password,
            environment: Environment::Local,
            ssh: None,
            last_opened_at: None,
            connect_timeout_secs: None,
            query_timeout_secs: None,
            attached_databases: Vec::new(),
        }
    }

    fn index(entries: &[(Uuid, Option<&str>, bool)]) -> ConnectionOrganizationIndex {
        let mut index = ConnectionOrganizationIndex::default();
        for (id, group, favorite) in entries {
            let organization = ConnectionOrganization::new(*group, &[], *favorite).expect("valid organization");
            index.set(*id, organization).expect("set");
        }
        index
    }

    #[test]
    fn connections_without_a_group_render_as_one_ungrouped_section() {
        let first = saved("alpha");
        let second = saved("beta");
        let arranged = vec![first.clone(), second.clone()];
        let tree = connection_group_tree(&arranged, &ConnectionOrganizationIndex::default());
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].path, None);
        assert_eq!(tree[0].connections.len(), 2);
    }

    #[test]
    fn nested_groups_render_as_a_tree_in_arranged_order() {
        let eu = saved("eu db");
        let us = saved("us db");
        let loose = saved("scratch");
        let index = index(&[
            (eu.id, Some("Production / EU"), false),
            (us.id, Some("Production / US"), false),
        ]);
        let arranged = vec![eu.clone(), us.clone(), loose.clone()];
        let tree = connection_group_tree(&arranged, &index);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].path.as_deref(), Some("Production"));
        assert_eq!(tree[0].children.len(), 2);
        assert_eq!(tree[0].children[0].path.as_deref(), Some("Production / EU"));
        assert_eq!(tree[0].children[1].path.as_deref(), Some("Production / US"));
        assert_eq!(tree[1].path, None);
        assert_eq!(tree[1].connections[0].id, loose.id);
    }

    #[test]
    fn collapsing_a_parent_hides_its_nested_children() {
        let eu = saved("eu db");
        let us = saved("us db");
        let index = index(&[
            (eu.id, Some("Production / EU"), false),
            (us.id, Some("Production / US"), false),
        ]);
        let tree = connection_group_tree(&[eu, us], &index);
        let mut collapsed = HashSet::new();
        collapsed.insert("Production".to_owned());
        assert_eq!(
            visible_group_paths(&tree, &collapsed),
            vec![Some("Production".to_owned())]
        );
        assert_eq!(
            visible_group_paths(&tree, &HashSet::new()),
            vec![
                Some("Production".to_owned()),
                Some("Production / EU".to_owned()),
                Some("Production / US".to_owned()),
            ]
        );
    }

    #[test]
    fn an_empty_arrangement_renders_no_sections() {
        assert!(connection_group_tree(&[], &ConnectionOrganizationIndex::default()).is_empty());
    }

    #[test]
    fn recent_connections_are_limited_and_ordered_by_successful_open_time() {
        let mut connections = [
            saved("old"),
            saved("new"),
            saved("middle"),
            saved("other"),
            saved("never"),
        ];
        for (connection, timestamp) in connections.iter_mut().zip([1, 4, 3, 2]) {
            connection.last_opened_at = Some(chrono::DateTime::from_timestamp(timestamp, 0).unwrap());
        }
        let names: Vec<_> = recent_connections(&connections)
            .into_iter()
            .map(|connection| connection.name.as_str())
            .collect();
        assert_eq!(names, ["new", "middle", "other"]);
    }

    #[test]
    fn recent_connection_details_make_production_visible() {
        assert_eq!(
            recent_connection_detail("PostgreSQL", Environment::Prod),
            "PostgreSQL · Production"
        );
    }
}
