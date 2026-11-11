use bookie_core::TableInfo;
use relm4::adw;
use relm4::gtk::prelude::*;
use relm4::{ComponentSender, gtk};
use std::time::{Duration, Instant};

use super::App;
use crate::services::sidebar_tree::{CollapseState, NodeKind, Reveal, build_nodes, reveal_target};
use crate::ui::sidebar_row::{GroupInit, SchemaCatalogActions, SidebarObjectKind, SidebarRowInit};

fn reveal_sidebar_unless_overlay(split_view: &adw::OverlaySplitView, generation: &std::rc::Rc<std::cell::Cell<u32>>) {
    reveal_with(split_view, generation, show_unless_overlay);
}

fn reveal_with(
    split_view: &adw::OverlaySplitView,
    generation: &std::rc::Rc<std::cell::Cell<u32>>,
    show: fn(&adw::OverlaySplitView),
) {
    let wanted = generation.get().wrapping_add(1);
    generation.set(wanted);
    if split_view.width() > 0 {
        show(split_view);
        return;
    }
    let generation = generation.clone();
    split_view.add_tick_callback(move |split_view, _| {
        if generation.get() != wanted {
            return relm4::gtk::glib::ControlFlow::Break;
        }
        if split_view.width() == 0 {
            return relm4::gtk::glib::ControlFlow::Continue;
        }
        show(split_view);
        relm4::gtk::glib::ControlFlow::Break
    });
}

fn sidebar_follows_connection(split_view: &adw::OverlaySplitView, toggle: &relm4::gtk::ToggleButton, connected: bool) {
    split_view.set_pin_sidebar(!connected);
    toggle.set_sensitive(connected);
}

fn show_unless_overlay(split_view: &adw::OverlaySplitView) {
    split_view.set_show_sidebar(!split_view.is_collapsed());
}

fn show_sidebar(split_view: &adw::OverlaySplitView, generation: &std::rc::Rc<std::cell::Cell<u32>>) {
    generation.set(generation.get().wrapping_add(1));
    split_view.set_show_sidebar(true);
}

pub(super) fn hide_sidebar(split_view: &adw::OverlaySplitView, generation: &std::rc::Rc<std::cell::Cell<u32>>) {
    generation.set(generation.get().wrapping_add(1));
    split_view.set_show_sidebar(false);
}

impl App {
    pub(super) fn hidden_schema_names(&self) -> Vec<String> {
        let mut schemas: Vec<_> = self.sidebar_hidden_schemas.borrow().iter().cloned().collect();
        schemas.sort();
        schemas
    }

    pub(super) fn refresh_sidebar(&self, sender: relm4::ComponentSender<App>) {
        if let Some(origin) = crate::services::catalog::CatalogOrigin::capture(self.connection_id, &self.database) {
            self.on_schema_changed(origin, None, None, sender);
        }
    }

    pub(super) fn show_sidebar_for_new_connection(&self) {
        self.set_sidebar_connected(true);
        reveal_sidebar_unless_overlay(&self.split_view, &self.sidebar_reveal);
    }

    pub(super) fn set_sidebar_connected(&self, connected: bool) {
        sidebar_follows_connection(&self.split_view, &self.sidebar_toggle, connected);
    }

    pub(super) fn repopulate_sidebar(&mut self, tables: &[TableInfo], _sender: ComponentSender<App>) {
        self.sidebar_tables = tables.to_vec();
        let mut nodes = build_nodes(tables, &self.sidebar_views);
        let catalog_kinds = self
            .current_driver_id
            .as_deref()
            .and_then(|id| self.registry.get(id))
            .map_or(&[][..], |driver| driver.capabilities().catalog_object_kinds);
        crate::services::sidebar_tree::append_catalog_entries_with(&mut nodes, catalog_kinds, &self.sidebar_catalog);
        let schema_filter_enabled = self.current_driver_id.as_deref() == Some("postgres");
        let timeout =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let column_source =
            crate::ui::sidebar_columns::ColumnSource::capture(self.connection_id, self.database.clone(), timeout);
        *self.sidebar_nodes.borrow_mut() = nodes;
        let mut render_context = self.sidebar_render_context.borrow_mut();
        render_context.driver_id = self.current_driver_id.clone().unwrap_or_default();
        render_context.schema_filter_enabled = schema_filter_enabled;
        render_context.column_source = column_source;
        drop(render_context);
        self.schema_filter_button.set_visible(schema_filter_enabled);
        self.refresh_sidebar_model();
        self.sync_sidebar_fold_button();
        self.sync_sidebar_selection();
    }

    pub(super) fn refresh_sidebar_model(&self) {
        let started = Instant::now();
        let nodes = self.sidebar_nodes.borrow();
        let sidebar_nodes = nodes.len();
        let included = crate::services::sidebar_tree::visible_nodes_with_hidden_schemas(
            &nodes,
            &CollapseState::default(),
            &self.table_search.text(),
            &self.sidebar_hidden_schemas.borrow(),
        );
        super::init_sidebar::disconnect_expansion_handlers(&self.sidebar_expansion_handlers);
        self.sidebar_model.replace_nodes(&nodes, &included);
        let model_build_ms = started.elapsed().as_secs_f64() * 1000.0;
        let row_factories = self.sidebar_row_factories.clone();
        gtk::glib::timeout_add_local_once(Duration::from_millis(250), move || {
            tracing::debug!(
                sidebar_nodes,
                tree_model_build_ms = model_build_ms,
                created_list_items = row_factories.borrow().len(),
                "sidebar tree model ready"
            );
        });
        self.sync_sidebar_selection();
    }

    pub(super) fn sidebar_group_expansion_changed(&mut self, key: &str, expanded: bool) {
        if !self.table_search.text().trim().is_empty() {
            return;
        }
        let is_collapsed = self.sidebar_collapsed.borrow().is_collapsed(key);
        if is_collapsed == expanded {
            self.sidebar_collapsed.borrow_mut().toggle(key);
        }
        self.sync_sidebar_fold_button();
        self.persist_workspace_state();
    }

    pub(super) fn toggle_all_sidebar_groups(&mut self) {
        if !self.table_search.text().is_empty() {
            return;
        }
        let collapse = {
            let nodes = self.sidebar_nodes.borrow();
            let mut collapsed = self.sidebar_collapsed.borrow_mut();
            if collapsed.is_all_collapsed(&nodes) {
                collapsed.expand_all();
                false
            } else {
                collapsed.collapse_all(&nodes);
                true
            }
        };
        let mut position = 0;
        while position < self.sidebar_model.tree().n_items() {
            let Some(row) = self.sidebar_model.tree().row(position) else {
                break;
            };
            let node = crate::ui::sidebar_tree_model::SidebarTreeModel::node_index(&row)
                .and_then(|index| self.sidebar_nodes.borrow().get(index).cloned());
            if node.is_some_and(|node| crate::ui::app::init_sidebar::is_expandable(&node)) {
                row.set_expanded(!collapse);
            }
            position += 1;
        }
        self.sync_sidebar_fold_button();
        self.persist_workspace_state();
    }

    pub(super) fn sync_sidebar_fold_button(&self) {
        let all_collapsed = self
            .sidebar_collapsed
            .borrow()
            .is_all_collapsed(&self.sidebar_nodes.borrow());
        let (icon, label) = if all_collapsed {
            ("go-down-symbolic", bookie_app_services::tr!("Expand all"))
        } else {
            ("go-up-symbolic", bookie_app_services::tr!("Collapse all"))
        };
        self.sidebar_fold_button.set_icon_name(icon);
        self.sidebar_fold_button.set_tooltip_text(Some(&label));
        self.sidebar_fold_button
            .update_property(&[relm4::gtk::accessible::Property::Label(&label)]);
        self.sidebar_fold_button
            .set_visible(!self.sidebar_nodes.borrow().is_empty());
    }

    pub(super) fn show_catalog(&self, kind: Option<bookie_core::CatalogObjectKind>) {
        crate::ui::catalog_dialog::present(
            self.window.upcast_ref::<relm4::gtk::Window>(),
            self.connection_id,
            &self.database,
            &self.preferences,
            self.catalog_dialog_cancel.borrow().clone(),
            crate::ui::catalog_dialog::CatalogRequest {
                initial_kind: kind,
                hidden_schemas: self.hidden_schema_names(),
                can_restart_sequences: self
                    .current_driver_id
                    .as_deref()
                    .and_then(|id| self.registry.get(id))
                    .is_some_and(|driver| driver.capabilities().supports_sequence_restart),
            },
        );
    }

    pub(super) fn on_reveal_in_sidebar(&mut self) {
        let Some((schema, table)) = self.selected_browse_slot_table() else {
            self.show_toast(&bookie_app_services::tr!("Open a table to reveal it in the sidebar."));
            return;
        };
        let plan = reveal_target(
            &self.sidebar_nodes.borrow(),
            &self.sidebar_collapsed.borrow(),
            &self.table_search.text(),
            &self.sidebar_hidden_schemas.borrow(),
            schema.as_deref(),
            &table,
        );
        match plan {
            Reveal::Missing => self.show_toast(
                &bookie_app_services::tr!("{table} is not in the sidebar. Refresh the connection if it is new.")
                    .replace("{table}", &table),
            ),
            Reveal::HiddenBySchemaFilter => self.show_toast(
                &bookie_app_services::tr!("The schema of {table} is hidden. Show it from the schema filter.")
                    .replace("{table}", &table),
            ),
            Reveal::Show {
                index,
                uncollapse,
                clear_search,
            } => {
                show_sidebar(&self.split_view, &self.sidebar_reveal);
                if clear_search {
                    self.table_search.set_text("");
                }
                for key in &uncollapse {
                    if self.sidebar_collapsed.borrow().is_collapsed(key) {
                        self.sidebar_collapsed.borrow_mut().toggle(key);
                    }
                }
                self.refresh_sidebar_model();
                self.focus_sidebar_row(index);
            }
        }
    }

    fn focus_sidebar_row(&mut self, index: usize) {
        for position in 0..self.sidebar_model.tree().n_items() {
            let Some(row) = self.sidebar_model.tree().row(position) else {
                continue;
            };
            if crate::ui::sidebar_tree_model::SidebarTreeModel::node_index(&row) == Some(index) {
                self.sidebar_selection.set_selected(position);
                self.sidebar_list.grab_focus();
                return;
            }
        }
    }

    pub(super) fn restore_sidebar_state(&self) {
        let state = self
            .workspace_target()
            .and_then(|(id, database, _)| self.workspace.load_database(id, &database));
        *self.sidebar_collapsed.borrow_mut() = CollapseState::from_keys(
            state
                .as_ref()
                .map(|state| state.collapsed_groups.clone())
                .unwrap_or_default(),
        );
        *self.sidebar_hidden_schemas.borrow_mut() = state
            .map(|state| state.hidden_schemas.into_iter().collect())
            .unwrap_or_default();
    }

    pub(super) fn hide_sidebar_schema(&mut self, schema: &str, sender: &relm4::ComponentSender<App>) {
        if schema.is_empty() || schema.len() > 256 {
            return;
        }
        let is_schema = self
            .sidebar_nodes
            .borrow()
            .iter()
            .any(|node| matches!(node.kind, NodeKind::Schema) && node.schema.as_deref() == Some(schema));
        if !is_schema {
            return;
        }
        self.sidebar_hidden_schemas.borrow_mut().insert(schema.to_string());
        self.reset_sidebar_catalog();
        self.refresh_schema_filter(sender);
        self.persist_workspace_state();
        self.refresh_sidebar(sender.clone());
    }

    pub(super) fn show_sidebar_schema(&mut self, schema: &str, sender: &relm4::ComponentSender<App>) {
        self.sidebar_hidden_schemas.borrow_mut().remove(schema);
        self.reset_sidebar_catalog();
        self.refresh_schema_filter(sender);
        self.persist_workspace_state();
        self.refresh_sidebar(sender.clone());
    }

    pub(super) fn show_all_sidebar_schemas(&mut self, sender: &relm4::ComponentSender<App>) {
        self.sidebar_hidden_schemas.borrow_mut().clear();
        self.reset_sidebar_catalog();
        self.refresh_schema_filter(sender);
        self.persist_workspace_state();
        self.refresh_sidebar(sender.clone());
    }

    pub(super) fn rebuild_schema_filter_popover(&self, sender: &relm4::ComponentSender<App>) {
        while let Some(child) = self.schema_filter_list.first_child() {
            self.schema_filter_list.remove(&child);
        }
        let heading = relm4::gtk::Label::builder()
            .label(bookie_app_services::tr!("Hidden schemas"))
            .xalign(0.0)
            .build();
        self.schema_filter_list.append(&heading);
        let mut hidden: Vec<String> = self.sidebar_hidden_schemas.borrow().iter().cloned().collect();
        hidden.sort();
        if hidden.is_empty() {
            let empty = relm4::gtk::Label::builder()
                .label(bookie_app_services::tr!("No schemas are hidden"))
                .xalign(0.0)
                .build();
            self.schema_filter_list.append(&empty);
            return;
        }
        let sender_for_all = sender.clone();
        let show_all = relm4::gtk::Button::with_label(&bookie_app_services::tr!("Show all schemas"));
        show_all.connect_clicked(move |_| sender_for_all.input(super::AppMsg::ShowAllSidebarSchemas));
        self.schema_filter_list.append(&show_all);
        for schema in hidden {
            let label = bookie_app_services::tr!("Show {schema}").replace("{schema}", &schema);
            let button = relm4::gtk::Button::with_label(&label);
            let sender_for_schema = sender.clone();
            button.connect_clicked(move |_| sender_for_schema.input(super::AppMsg::ShowSidebarSchema(schema.clone())));
            self.schema_filter_list.append(&button);
        }
    }

    fn refresh_schema_filter(&self, sender: &relm4::ComponentSender<App>) {
        self.refresh_sidebar_model();
        self.rebuild_schema_filter_popover(sender);
    }
}

fn sidebar_group_label(node: &crate::services::sidebar_tree::TreeNode) -> String {
    use crate::services::sidebar_tree::{GroupKind, NodeKind};
    match &node.kind {
        NodeKind::Schema => node.schema.clone().unwrap_or_default(),
        NodeKind::Group(GroupKind::Tables) => bookie_app_services::tr!("Tables").to_string(),
        NodeKind::Group(GroupKind::Views) => bookie_app_services::tr!("Views").to_string(),
        NodeKind::Group(GroupKind::Catalog) => bookie_app_services::tr!("Catalog").to_string(),
        NodeKind::Catalog(kind) | NodeKind::CatalogLoaded(kind) => crate::ui::catalog_dialog::kind_label(*kind),
        NodeKind::CatalogObject { schema, name, .. } => schema
            .as_deref()
            .map_or_else(|| name.clone(), |schema| format!("{schema}.{name}")),
        NodeKind::CatalogMore(_) => bookie_app_services::tr!("Show all in Catalog…").to_string(),
        NodeKind::Table { name, .. } | NodeKind::View { name, .. } => name.clone(),
    }
}

pub(super) fn row_for(
    node: &crate::services::sidebar_tree::TreeNode,
    collapsed: &crate::services::sidebar_tree::CollapseState,
    searching: bool,
    schema_filter_enabled: bool,
    driver_id: &str,
    column_source: Option<crate::ui::sidebar_columns::ColumnSource>,
) -> SidebarRowInit {
    use crate::services::sidebar_tree::NodeKind;
    let create_schema = bookie_core::sql_ddl::supports_create_schema(driver_id);
    let (kind, group) = row_kind_and_group(node, collapsed, searching, create_schema);
    let name = match &node.kind {
        NodeKind::Table { name, .. } | NodeKind::View { name, .. } => name.clone(),
        _ => node.key.clone(),
    };
    let schema_name = node.schema.as_deref().unwrap_or_default();
    SidebarRowInit {
        info: TableInfo {
            schema: node.schema.clone(),
            name,
        },
        kind,
        depth: node.depth,
        group,
        column_source,
        schema_actions: SchemaCatalogActions {
            create: create_schema,
            drop: bookie_core::sql_ddl::supports_drop_schema(driver_id)
                && !schema_name.is_empty()
                && !bookie_core::sql_ddl::protects_schema(driver_id, schema_name),
            hide: schema_filter_enabled,
        },
    }
}

fn row_kind_and_group(
    node: &crate::services::sidebar_tree::TreeNode,
    collapsed: &crate::services::sidebar_tree::CollapseState,
    searching: bool,
    create_schema: bool,
) -> (SidebarObjectKind, Option<GroupInit>) {
    use crate::services::sidebar_tree::{GroupKind, NodeKind};
    match &node.kind {
        NodeKind::Table { .. } => (SidebarObjectKind::Table, None),
        NodeKind::View { .. } => (SidebarObjectKind::View, None),
        NodeKind::Catalog(_) | NodeKind::CatalogObject { .. } | NodeKind::CatalogMore(_) => (
            SidebarObjectKind::Catalog,
            Some(GroupInit {
                label: sidebar_group_label(node),
                count: 0,
                collapsed: false,
                actions: false,
                entry: true,
                new_schema: false,
            }),
        ),
        NodeKind::CatalogLoaded(_) => (
            SidebarObjectKind::Catalog,
            Some(GroupInit {
                label: sidebar_group_label(node),
                count: node.count,
                collapsed: !searching && collapsed.is_collapsed(&node.key),
                actions: false,
                entry: false,
                new_schema: false,
            }),
        ),
        NodeKind::Schema | NodeKind::Group(_) => {
            let is_schema = matches!(node.kind, NodeKind::Schema) && node.schema.is_some();
            let is_tables = matches!(node.kind, NodeKind::Group(GroupKind::Tables));
            (
                if is_schema {
                    SidebarObjectKind::Schema
                } else {
                    SidebarObjectKind::Group
                },
                Some(GroupInit {
                    label: sidebar_group_label(node),
                    count: node.count,
                    collapsed: !searching && collapsed.is_collapsed(&node.key),
                    actions: is_tables,
                    entry: false,
                    new_schema: is_tables && create_schema && node.depth == 0,
                }),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn the_sidebar_stays_closed_in_a_narrow_window_and_never_opens_empty() {
        relm4::adw::init().unwrap();
        let split_view = adw::OverlaySplitView::new();
        let toggle = relm4::gtk::ToggleButton::new();
        sidebar_follows_connection(&split_view, &toggle, false);
        split_view.set_show_sidebar(false);
        split_view.set_collapsed(true);
        split_view.set_collapsed(false);
        assert!(
            !split_view.shows_sidebar(),
            "widening without a connection keeps the empty sidebar closed"
        );
        assert!(!toggle.is_sensitive());

        sidebar_follows_connection(&split_view, &toggle, true);
        split_view.set_collapsed(true);
        show_unless_overlay(&split_view);
        assert!(
            !split_view.shows_sidebar(),
            "connecting in a narrow window does not cover the workspace"
        );
        assert!(toggle.is_sensitive());
        split_view.set_collapsed(false);
        assert!(
            split_view.shows_sidebar(),
            "widening while connected docks the sidebar again"
        );
    }
}

#[cfg(test)]
mod explicit_reveal_tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn an_explicit_reveal_opens_the_sidebar_in_a_narrow_window() {
        relm4::adw::init().unwrap();
        let split = adw::OverlaySplitView::builder()
            .collapsed(true)
            .show_sidebar(false)
            .build();
        let generation = std::rc::Rc::new(std::cell::Cell::new(3));
        show_sidebar(&split, &generation);
        assert!(split.shows_sidebar());
        assert_eq!(generation.get(), 4);
        hide_sidebar(&split, &generation);
        assert!(!split.shows_sidebar());
    }
}

#[cfg(test)]
mod sidebar_label_tests {
    use super::*;
    use crate::services::sidebar_tree::{GroupKind, NodeKind, build_nodes};

    #[test]
    fn group_labels_are_localized_by_the_app_adapter() {
        let nodes = build_nodes(
            &[
                TableInfo {
                    schema: Some("archive".into()),
                    name: "old_users".into(),
                },
                TableInfo {
                    schema: Some("public".into()),
                    name: "users".into(),
                },
            ],
            &[TableInfo {
                schema: Some("public".into()),
                name: "recent_users".into(),
            }],
        );
        let table_group = nodes
            .iter()
            .find(|node| node.kind == NodeKind::Group(GroupKind::Tables))
            .expect("table group exists");
        let view_group = nodes
            .iter()
            .find(|node| node.kind == NodeKind::Group(GroupKind::Views))
            .expect("view group exists");

        assert_eq!(
            sidebar_group_label(table_group),
            bookie_app_services::tr!("Tables").to_string()
        );
        assert_eq!(
            sidebar_group_label(view_group),
            bookie_app_services::tr!("Views").to_string()
        );
    }

    #[test]
    fn catalog_entries_get_a_catalog_row_kind_and_the_dialog_label() {
        use crate::services::sidebar_tree::append_catalog_entries;
        let mut nodes = build_nodes(&[], &[]);
        append_catalog_entries(&mut nodes, &[bookie_core::CatalogObjectKind::Role]);
        let group = nodes
            .iter()
            .find(|node| node.kind == NodeKind::Group(GroupKind::Catalog))
            .unwrap();
        let entry = nodes
            .iter()
            .find(|node| node.kind == NodeKind::Catalog(bookie_core::CatalogObjectKind::Role))
            .unwrap();

        assert_eq!(
            sidebar_group_label(group),
            bookie_app_services::tr!("Catalog").to_string()
        );
        let init = row_for(entry, &Default::default(), false, false, "postgres", None);
        assert_eq!(init.kind, SidebarObjectKind::Catalog);
        let group_init = init.group.unwrap();
        assert!(group_init.entry);
        assert_eq!(group_init.label, bookie_app_services::tr!("Roles").to_string());
    }

    #[test]
    fn loaded_catalog_kinds_are_branches_and_their_objects_are_leaf_entries() {
        use crate::services::sidebar_tree::append_catalog_entries_with;
        let kind = bookie_core::CatalogObjectKind::Routine;
        let object = bookie_core::CatalogObject {
            kind,
            schema: Some("public".into()),
            name: "calc_tax".into(),
            detail: None,
        };
        let mut nodes = build_nodes(&[], &[]);
        append_catalog_entries_with(
            &mut nodes,
            &[kind],
            &std::collections::HashMap::from([(kind, vec![object])]),
        );
        let branch = nodes
            .iter()
            .find(|node| node.kind == NodeKind::CatalogLoaded(kind))
            .unwrap();
        let leaf = nodes
            .iter()
            .find(|node| matches!(node.kind, NodeKind::CatalogObject { .. }))
            .unwrap();

        let branch_init = row_for(branch, &Default::default(), false, false, "postgres", None)
            .group
            .unwrap();
        assert!(!branch_init.entry);
        assert_eq!(branch_init.count, 1);
        let leaf_init = row_for(leaf, &Default::default(), false, false, "postgres", None);
        assert_eq!(leaf_init.depth, 2);
        let leaf_group = leaf_init.group.unwrap();
        assert!(leaf_group.entry);
        assert_eq!(leaf_group.label, "public.calc_tax");
    }
}
