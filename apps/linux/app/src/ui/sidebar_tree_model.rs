use bookie_app_services::sidebar_tree::{NodeKind, TreeNode};
use relm4::gtk::prelude::*;
use relm4::gtk::{self, gio, glib};

struct ModelNode {
    index: usize,
    children: Option<gio::ListStore>,
}

pub(crate) struct SidebarTreeModel {
    roots: gio::ListStore,
    tree: gtk::TreeListModel,
}

impl SidebarTreeModel {
    pub(crate) fn new(nodes: &[TreeNode]) -> Self {
        let roots = gio::ListStore::new::<glib::BoxedAnyObject>();
        let tree = gtk::TreeListModel::new(roots.clone(), false, false, |object| {
            let item = object.downcast_ref::<glib::BoxedAnyObject>()?;
            let model = item.try_borrow::<ModelNode>().ok()?;
            model.children.as_ref().map(|children| children.clone().upcast())
        });
        let model = Self { roots, tree };
        model.replace_nodes(nodes, &(0..nodes.len()).collect::<Vec<_>>());
        model
    }

    pub(crate) fn replace_nodes(&self, nodes: &[TreeNode], included: &[usize]) {
        let mut parents = Vec::<(u8, gio::ListStore)>::new();
        let mut roots = Vec::new();
        for &index in included {
            let Some(node) = nodes.get(index) else {
                continue;
            };
            while parents.last().is_some_and(|(depth, _)| *depth >= node.depth) {
                parents.pop();
            }
            let children = is_branch(&node.kind).then(gio::ListStore::new::<glib::BoxedAnyObject>);
            let model_node = glib::BoxedAnyObject::new(ModelNode {
                index,
                children: children.clone(),
            });
            if let Some((_, parent)) = parents.last() {
                parent.append(&model_node);
            } else {
                roots.push(model_node);
            }
            if let Some(children) = children {
                parents.push((node.depth, children));
            }
        }
        self.roots.splice(0, self.roots.n_items(), &roots);
    }

    pub(crate) fn roots(&self) -> &gio::ListStore {
        &self.roots
    }

    pub(crate) fn tree(&self) -> &gtk::TreeListModel {
        &self.tree
    }

    pub(crate) fn node_index(row: &gtk::TreeListRow) -> Option<usize> {
        let item = row.item()?;
        let item = item.downcast_ref::<glib::BoxedAnyObject>()?;
        Some(item.try_borrow::<ModelNode>().ok()?.index)
    }
}

fn is_branch(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Schema | NodeKind::Group(_) | NodeKind::Catalog(_) | NodeKind::CatalogLoaded(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_app_services::sidebar_tree::build_nodes;
    use bookie_core::TableInfo;

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn tree_model_keeps_catalog_nodes_under_their_schema_without_flattening_children() {
        relm4::gtk::init().unwrap();
        let tables = [
            table("alpha", "first"),
            table("alpha", "second"),
            table("beta", "third"),
        ];
        let nodes = build_nodes(&tables, &[]);
        let model = SidebarTreeModel::new(&nodes);

        assert_eq!(model.roots().n_items(), 2);
        assert_eq!(model.tree().n_items(), 2);
        let alpha = model.tree().row(0).unwrap();
        assert!(!alpha.is_expanded());
        alpha.set_expanded(true);
        assert_eq!(model.tree().n_items(), 3);
        let tables_group = model.tree().row(1).unwrap();
        assert!(tables_group.is_expandable());
        tables_group.set_expanded(true);
        assert_eq!(model.tree().n_items(), 5);
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn large_catalog_domain_nodes_do_not_become_list_rows_until_their_groups_expand() {
        relm4::gtk::init().unwrap();
        let tables: Vec<_> = (0..5_000)
            .map(|index| table(&format!("schema_{:02}", index / 100), &format!("table_{index}")))
            .collect();
        let nodes = build_nodes(&tables, &[]);
        let model = SidebarTreeModel::new(&nodes);

        assert_eq!(nodes.len(), 5_100);
        assert_eq!(model.roots().n_items(), 50);
        assert_eq!(model.tree().n_items(), 50);
        let mut position = 0;
        while position < model.tree().n_items() {
            let row = model.tree().row(position).unwrap();
            if row.is_expandable() {
                row.set_expanded(true);
            }
            position += 1;
        }
        assert_eq!(model.tree().n_items(), nodes.len() as u32);
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn gtk_list_view_realizes_a_bounded_fraction_of_a_large_expanded_table_group() {
        use std::cell::Cell;
        use std::rc::Rc;
        use std::time::Duration;

        relm4::gtk::init().unwrap();
        let tables: Vec<_> = (0..5_000)
            .map(|index| table("public", &format!("table_{index}")))
            .collect();
        let nodes = build_nodes(&tables, &[]);
        let model = SidebarTreeModel::new(&nodes);
        model.tree().row(0).unwrap().set_expanded(true);
        assert_eq!(model.tree().n_items(), 5_001);

        let created = Rc::new(Cell::new(0));
        let factory = gtk::SignalListItemFactory::new();
        let created_for_setup = created.clone();
        factory.connect_setup(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            created_for_setup.set(created_for_setup.get() + 1);
            item.set_child(Some(&gtk::Label::new(Some("table"))));
        });
        let selection = gtk::SingleSelection::new(Some(model.tree().clone()));
        let list = gtk::ListView::new(Some(selection), Some(factory));
        let scrolled = gtk::ScrolledWindow::builder()
            .child(&list)
            .hexpand(true)
            .vexpand(true)
            .build();
        let window = gtk::Window::builder()
            .default_width(360)
            .default_height(480)
            .child(&scrolled)
            .build();
        window.present();
        pump_main_context(Duration::from_millis(150));

        assert!(created.get() > 0);
        assert!(
            created.get() < model.tree().n_items() / 10,
            "realized {} rows for a {}-row model in {}x{} window",
            created.get(),
            model.tree().n_items(),
            window.width(),
            window.height()
        );
        window.close();
    }

    fn pump_main_context(duration: std::time::Duration) {
        let context = glib::MainContext::default();
        let deadline = std::time::Instant::now() + duration;
        while std::time::Instant::now() < deadline {
            while context.pending() {
                context.iteration(false);
            }
        }
    }

    fn table(schema: &str, name: &str) -> TableInfo {
        TableInfo {
            schema: Some(schema.into()),
            name: name.into(),
        }
    }
}
