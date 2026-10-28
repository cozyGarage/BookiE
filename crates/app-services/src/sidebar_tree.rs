use std::collections::{BTreeMap, HashSet};

use tablepro_core::{CatalogObjectKind, TableInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    Tables,
    Views,
    Catalog,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Schema,
    Group(GroupKind),
    Table { schema: Option<String>, name: String },
    View { schema: Option<String>, name: String },
    Catalog(CatalogObjectKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    pub depth: u8,
    pub key: String,
    pub kind: NodeKind,
    pub schema: Option<String>,
    pub count: usize,
    parent: Option<usize>,
}

impl TreeNode {
    fn is_branch(&self) -> bool {
        matches!(self.kind, NodeKind::Schema | NodeKind::Group(_))
    }

    fn qualified_name(&self) -> Option<String> {
        match &self.kind {
            NodeKind::Table { schema, name } | NodeKind::View { schema, name } => Some(match schema {
                Some(schema) => format!("{schema}.{name}"),
                None => name.clone(),
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CollapseState(HashSet<String>);

impl CollapseState {
    pub fn toggle(&mut self, key: &str) {
        if !self.0.remove(key) {
            self.0.insert(key.to_string());
        }
    }

    pub fn is_collapsed(&self, key: &str) -> bool {
        self.0.contains(key)
    }

    pub fn collapse_all(&mut self, nodes: &[TreeNode]) {
        self.0.extend(
            nodes
                .iter()
                .filter(|node| node.is_branch())
                .map(|node| node.key.clone()),
        );
    }

    pub fn expand_all(&mut self) {
        self.0.clear();
    }

    pub fn is_all_collapsed(&self, nodes: &[TreeNode]) -> bool {
        let mut branches = nodes.iter().filter(|node| node.is_branch()).peekable();
        branches.peek().is_some() && branches.all(|node| self.is_collapsed(&node.key))
    }

    pub fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.0.iter().cloned().collect();
        keys.sort();
        keys
    }

    pub fn from_keys(keys: impl IntoIterator<Item = String>) -> Self {
        Self(keys.into_iter().collect())
    }
}

pub fn build_nodes(tables: &[TableInfo], views: &[TableInfo]) -> Vec<TreeNode> {
    let mut schemas = BTreeMap::<Option<String>, (Vec<&TableInfo>, Vec<&TableInfo>)>::new();
    for table in tables {
        schemas.entry(table.schema.clone()).or_default().0.push(table);
    }
    for view in views {
        schemas.entry(view.schema.clone()).or_default().1.push(view);
    }
    let several_schemas = schemas.keys().filter(|schema| schema.is_some()).count() > 1;
    let mut nodes = Vec::new();
    for (schema, (schema_tables, schema_views)) in &schemas {
        let prefix = format!("schema:{}", key_part(schema.as_deref().unwrap_or_default()));
        let mut depth = 0;
        let parent = if several_schemas {
            let schema_index = nodes.len();
            nodes.push(TreeNode {
                depth,
                key: prefix.clone(),
                kind: NodeKind::Schema,
                schema: schema.clone(),
                count: schema_tables.len() + schema_views.len(),
                parent: None,
            });
            depth += 1;
            Some(schema_index)
        } else {
            None
        };
        push_group(&mut nodes, depth, &prefix, GroupKind::Tables, schema_tables, parent);
        push_group(&mut nodes, depth, &prefix, GroupKind::Views, schema_views, parent);
    }
    nodes
}

pub fn append_catalog_entries(nodes: &mut Vec<TreeNode>, kinds: &[CatalogObjectKind]) {
    if kinds.is_empty() {
        return;
    }
    let group_index = nodes.len();
    nodes.push(TreeNode {
        depth: 0,
        key: "catalog".to_string(),
        kind: NodeKind::Group(GroupKind::Catalog),
        schema: None,
        count: kinds.len(),
        parent: None,
    });
    for kind in kinds {
        nodes.push(TreeNode {
            depth: 1,
            key: format!("catalog/{}", kind.as_str()),
            kind: NodeKind::Catalog(*kind),
            schema: None,
            count: 0,
            parent: Some(group_index),
        });
    }
}

fn push_group(
    nodes: &mut Vec<TreeNode>,
    depth: u8,
    prefix: &str,
    group: GroupKind,
    objects: &[&TableInfo],
    parent: Option<usize>,
) {
    if objects.is_empty() {
        return;
    }
    let suffix = match group {
        GroupKind::Tables => "tables",
        GroupKind::Views => "views",
        GroupKind::Catalog => "catalog",
    };
    let group_index = nodes.len();
    nodes.push(TreeNode {
        depth,
        key: format!("{prefix}/{suffix}"),
        kind: NodeKind::Group(group),
        schema: objects.first().and_then(|object| object.schema.clone()),
        count: objects.len(),
        parent,
    });
    for object in objects {
        let (schema, name) = (object.schema.clone(), object.name.clone());
        nodes.push(TreeNode {
            depth: depth + 1,
            key: format!("{prefix}/{suffix}/{}", key_part(&name)),
            kind: match group {
                GroupKind::Tables | GroupKind::Catalog => NodeKind::Table { schema, name },
                GroupKind::Views => NodeKind::View { schema, name },
            },
            schema: object.schema.clone(),
            count: 0,
            parent: Some(group_index),
        });
    }
}

fn key_part(raw: &str) -> String {
    raw.replace('%', "%25").replace('/', "%2F").replace(':', "%3A")
}

fn ancestor_keys(node: &TreeNode) -> Vec<String> {
    let mut keys = Vec::new();
    let mut path = node.key.as_str();
    while let Some((parent, _)) = path.rsplit_once('/') {
        keys.push(parent.to_string());
        path = parent;
    }
    keys
}

fn has_collapsed_ancestor(node: &TreeNode, nodes: &[TreeNode], collapsed: &CollapseState) -> bool {
    let mut parent = node.parent;
    while let Some(index) = parent {
        if collapsed.is_collapsed(&nodes[index].key) {
            return true;
        }
        parent = nodes[index].parent;
    }
    false
}

#[cfg(test)]
pub fn visible_nodes(nodes: &[TreeNode], collapsed: &CollapseState, query: &str) -> Vec<usize> {
    visible_nodes_with_hidden_schemas(nodes, collapsed, query, &HashSet::new())
}

pub fn visible_nodes_with_hidden_schemas(
    nodes: &[TreeNode],
    collapsed: &CollapseState,
    query: &str,
    hidden_schemas: &HashSet<String>,
) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return (0..nodes.len())
            .filter(|index| schema_is_visible(&nodes[*index], hidden_schemas))
            .filter(|index| !has_collapsed_ancestor(&nodes[*index], nodes, collapsed))
            .collect();
    }
    let mut shown = vec![false; nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        if !schema_is_visible(node, hidden_schemas)
            || !node
                .qualified_name()
                .is_some_and(|name| name.to_lowercase().contains(&query))
        {
            continue;
        }
        shown[index] = true;
        let mut parent = node.parent;
        while let Some(parent_index) = parent {
            if schema_is_visible(&nodes[parent_index], hidden_schemas) {
                shown[parent_index] = true;
            }
            parent = nodes[parent_index].parent;
        }
    }
    shown
        .into_iter()
        .enumerate()
        .filter_map(|(index, is_shown)| is_shown.then_some(index))
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Reveal {
    Missing,
    HiddenBySchemaFilter,
    Show {
        index: usize,
        uncollapse: Vec<String>,
        clear_search: bool,
    },
}

pub fn reveal_target(
    nodes: &[TreeNode],
    collapsed: &CollapseState,
    query: &str,
    hidden_schemas: &HashSet<String>,
    schema: Option<&str>,
    name: &str,
) -> Reveal {
    let Some(index) = nodes.iter().position(|node| match &node.kind {
        NodeKind::Table {
            schema: node_schema,
            name: node_name,
        }
        | NodeKind::View {
            schema: node_schema,
            name: node_name,
        } => node_name == name && node_schema.as_deref() == schema,
        _ => false,
    }) else {
        return Reveal::Missing;
    };
    let node = &nodes[index];
    if !schema_is_visible(node, hidden_schemas) {
        return Reveal::HiddenBySchemaFilter;
    }
    let query = query.trim().to_lowercase();
    let matches_search = node
        .qualified_name()
        .is_some_and(|name| name.to_lowercase().contains(&query));
    if !query.is_empty() && matches_search {
        return Reveal::Show {
            index,
            uncollapse: Vec::new(),
            clear_search: false,
        };
    }
    let mut uncollapse: Vec<String> = ancestor_keys(node)
        .into_iter()
        .filter(|key| collapsed.is_collapsed(key))
        .collect();
    uncollapse.reverse();
    Reveal::Show {
        index,
        uncollapse,
        clear_search: !query.is_empty(),
    }
}

fn schema_is_visible(node: &TreeNode, hidden_schemas: &HashSet<String>) -> bool {
    node.schema
        .as_ref()
        .is_none_or(|schema| !hidden_schemas.contains(schema))
}

#[cfg(test)]
#[path = "sidebar_tree_tests.rs"]
mod tests;
