use super::*;

fn info(schema: Option<&str>, name: &str) -> TableInfo {
    TableInfo {
        schema: schema.map(str::to_string),
        name: name.to_string(),
    }
}

fn labels(nodes: &[TreeNode], indexes: &[usize]) -> Vec<String> {
    indexes
        .iter()
        .map(|index| format!("{}{}", "  ".repeat(nodes[*index].depth as usize), label(&nodes[*index])))
        .collect()
}

fn label(node: &TreeNode) -> String {
    match &node.kind {
        NodeKind::Schema => node.schema.clone().unwrap_or_default(),
        NodeKind::Group(GroupKind::Tables) => "Tables".into(),
        NodeKind::Group(GroupKind::Views) => "Views".into(),
        NodeKind::Table { name, .. } | NodeKind::View { name, .. } => name.clone(),
    }
}

fn all(nodes: &[TreeNode]) -> Vec<usize> {
    (0..nodes.len()).collect()
}

#[test]
fn searching_a_large_catalog_keeps_every_matching_object_under_its_parents() {
    let tables: Vec<_> = (0..50_000)
        .map(|index| info(Some(&format!("schema_{}", index % 100)), &format!("table_{index}")))
        .collect();
    let nodes = build_nodes(&tables, &[]);

    let visible = visible_nodes_with_hidden_schemas(&nodes, &CollapseState::default(), "table_", &HashSet::new());

    assert_eq!(visible, all(&nodes));
    assert_eq!(nodes.len(), 50_200);
}

#[test]
fn a_single_schema_shows_groups_at_the_top_without_a_schema_row() {
    let nodes = build_nodes(&[info(None, "users"), info(None, "orders")], &[info(None, "recent")]);
    assert_eq!(
        labels(&nodes, &all(&nodes)),
        ["Tables", "  users", "  orders", "Views", "  recent"]
    );
}

#[test]
fn removing_other_schemas_keeps_the_remaining_catalog_visible_after_a_saved_schema_collapse() {
    let collapsed = CollapseState::from_keys(["schema:public".to_string()]);
    let nodes = build_nodes(&[info(Some("public"), "users")], &[info(Some("public"), "recent")]);

    assert_eq!(
        labels(&nodes, &visible_nodes(&nodes, &collapsed, "")),
        ["Tables", "  users", "Views", "  recent"]
    );
}

#[test]
fn several_schemas_nest_groups_and_objects_under_each_schema() {
    let nodes = build_nodes(
        &[info(Some("public"), "users"), info(Some("audit"), "events")],
        &[info(Some("public"), "recent")],
    );
    assert_eq!(
        labels(&nodes, &all(&nodes)),
        [
            "audit",
            "  Tables",
            "    events",
            "public",
            "  Tables",
            "    users",
            "  Views",
            "    recent"
        ]
    );
}

#[test]
fn a_hidden_schema_and_its_objects_are_removed_from_sidebar_results() {
    let nodes = build_nodes(
        &[info(Some("audit"), "events"), info(Some("public"), "users")],
        &[info(Some("audit"), "recent_events")],
    );
    let hidden = HashSet::from(["audit".to_string()]);
    let visible = visible_nodes_with_hidden_schemas(&nodes, &CollapseState::default(), "", &hidden);

    assert_eq!(labels(&nodes, &visible), ["public", "  Tables", "    users"]);
}

#[test]
fn a_search_cannot_reveal_a_hidden_schema() {
    let nodes = build_nodes(&[info(Some("private"), "secrets")], &[]);
    let hidden = HashSet::from(["private".to_string()]);

    assert!(visible_nodes_with_hidden_schemas(&nodes, &CollapseState::default(), "secrets", &hidden).is_empty());
}

#[test]
fn groups_and_schemas_carry_how_many_objects_they_hold() {
    let tables = [info(Some("a"), "t1"), info(Some("a"), "t2"), info(Some("b"), "t3")];
    let views = [info(Some("a"), "v1")];
    let nodes = build_nodes(&tables, &views);
    let count_of = |key: &str| nodes.iter().find(|node| node.key == key).map(|node| node.count);
    assert_eq!(count_of("schema:a"), Some(3));
    assert_eq!(count_of("schema:a/tables"), Some(2));
    assert_eq!(count_of("schema:a/views"), Some(1));
    assert_eq!(count_of("schema:b/tables"), Some(1));
}

#[test]
fn a_schema_or_table_name_with_separators_cannot_collide_with_another_node() {
    let tables = [
        info(Some("a"), "t"),
        info(Some("a/b"), "u"),
        info(Some("x/tables"), "v"),
        info(Some("x"), "w"),
        info(Some("x"), "p/q"),
    ];
    let nodes = build_nodes(&tables, &[]);
    let keys: HashSet<&str> = nodes.iter().map(|node| node.key.as_str()).collect();
    assert_eq!(keys.len(), nodes.len());

    let mut collapsed = CollapseState::default();
    collapsed.toggle("schema:a");
    let shown = visible_nodes(&nodes, &collapsed, "");
    let hidden: Vec<String> = nodes
        .iter()
        .enumerate()
        .filter(|(index, _)| !shown.contains(index))
        .map(|(_, node)| node)
        .map(label)
        .collect();
    assert_eq!(hidden, vec!["Tables", "t"]);
}

#[test]
fn a_view_also_returned_as_a_table_appears_only_in_views() {
    let nodes = build_nodes(
        &[info(Some("db"), "items"), info(Some("db"), "recent")],
        &[info(Some("db"), "recent")],
    );
    assert_eq!(labels(&nodes, &all(&nodes)), ["Tables", "  items", "Views", "  recent"]);
}

#[test]
fn an_empty_group_is_left_out() {
    let nodes = build_nodes(&[], &[info(None, "only_view")]);
    assert_eq!(labels(&nodes, &all(&nodes)), ["Views", "  only_view"]);
}

#[test]
fn collapsing_a_group_hides_its_objects_but_keeps_the_group_to_expand_again() {
    let nodes = build_nodes(&[info(None, "users")], &[info(None, "recent")]);
    let mut collapsed = CollapseState::default();
    collapsed.toggle("schema:/tables");
    assert_eq!(
        labels(&nodes, &visible_nodes(&nodes, &collapsed, "")),
        ["Tables", "Views", "  recent"]
    );
    collapsed.toggle("schema:/tables");
    assert_eq!(visible_nodes(&nodes, &collapsed, "").len(), nodes.len());
}

#[test]
fn collapsing_a_schema_hides_everything_below_it() {
    let nodes = build_nodes(&[info(Some("a"), "t1"), info(Some("b"), "t2")], &[]);
    let mut collapsed = CollapseState::default();
    collapsed.toggle("schema:a");
    assert_eq!(
        labels(&nodes, &visible_nodes(&nodes, &collapsed, "")),
        ["a", "b", "  Tables", "    t2"]
    );
}

#[test]
fn a_search_shows_matches_with_their_parents_and_ignores_the_collapse_state() {
    let nodes = build_nodes(
        &[info(Some("public"), "users"), info(Some("audit"), "events")],
        &[info(Some("public"), "user_names")],
    );
    let mut collapsed = CollapseState::default();
    collapsed.toggle("schema:public");
    assert_eq!(
        labels(&nodes, &visible_nodes(&nodes, &collapsed, "user")),
        ["public", "  Tables", "    users", "  Views", "    user_names"]
    );
    assert_eq!(
        labels(&nodes, &visible_nodes(&nodes, &collapsed, "audit.ev")),
        ["audit", "  Tables", "    events"]
    );
    assert!(visible_nodes(&nodes, &collapsed, "zzz").is_empty());
}

#[test]
fn the_collapse_state_round_trips_through_its_saved_keys() {
    let mut collapsed = CollapseState::default();
    collapsed.toggle("schema:b");
    collapsed.toggle("schema:a/views");
    let restored = CollapseState::from_keys(collapsed.keys());
    assert!(restored.is_collapsed("schema:b") && restored.is_collapsed("schema:a/views"));
    assert!(!restored.is_collapsed("schema:a"));
    assert_eq!(collapsed.keys(), ["schema:a/views", "schema:b"]);
}

fn two_schemas() -> Vec<TreeNode> {
    build_nodes(
        &[info(Some("public"), "users"), info(Some("audit"), "events")],
        &[info(Some("public"), "recent")],
    )
}

fn key_of(nodes: &[TreeNode], expected_label: &str, depth: u8) -> String {
    nodes
        .iter()
        .find(|node| label(node) == expected_label && node.depth == depth)
        .map(|node| node.key.clone())
        .expect("a node with that label and depth")
}

#[test]
fn a_table_in_an_open_tree_needs_nothing_expanded() {
    let nodes = two_schemas();
    let plan = reveal_target(
        &nodes,
        &CollapseState::default(),
        "",
        &HashSet::new(),
        Some("public"),
        "users",
    );
    let Reveal::Show {
        index,
        uncollapse,
        clear_search,
    } = plan
    else {
        panic!("{plan:?}");
    };
    assert_eq!(label(&nodes[index]), "users");
    assert!(uncollapse.is_empty());
    assert!(!clear_search);
}

#[test]
fn collapsed_ancestors_are_expanded_outermost_first() {
    let nodes = two_schemas();
    let schema = key_of(&nodes, "public", 0);
    let group = "schema:public/tables".to_string();
    assert!(
        nodes.iter().any(|node| node.key == group),
        "the key format the test relies on"
    );
    let collapsed = CollapseState::from_keys([schema.clone(), group.clone()]);

    let plan = reveal_target(&nodes, &collapsed, "", &HashSet::new(), Some("public"), "users");

    let Reveal::Show { uncollapse, .. } = plan else {
        panic!("{plan:?}");
    };
    assert_eq!(uncollapse, [schema, group]);
}

#[test]
fn a_search_that_hides_the_table_is_cleared_and_one_that_shows_it_is_kept() {
    let nodes = two_schemas();
    let hides = reveal_target(
        &nodes,
        &CollapseState::default(),
        "events",
        &HashSet::new(),
        Some("public"),
        "users",
    );
    assert!(matches!(hides, Reveal::Show { clear_search: true, .. }), "{hides:?}");

    let collapsed = CollapseState::from_keys([key_of(&nodes, "public", 0)]);
    let shows = reveal_target(&nodes, &collapsed, "USER", &HashSet::new(), Some("public"), "users");
    assert!(
        matches!(&shows, Reveal::Show { clear_search: false, uncollapse, .. } if uncollapse.is_empty()),
        "a matching search already lists the table whatever is collapsed: {shows:?}"
    );
}

#[test]
fn a_table_in_a_hidden_schema_or_not_listed_is_reported_instead_of_revealed() {
    let nodes = two_schemas();
    let hidden = HashSet::from(["public".to_string()]);
    assert_eq!(
        reveal_target(&nodes, &CollapseState::default(), "", &hidden, Some("public"), "users"),
        Reveal::HiddenBySchemaFilter
    );
    assert_eq!(
        reveal_target(
            &nodes,
            &CollapseState::default(),
            "",
            &HashSet::new(),
            Some("public"),
            "missing"
        ),
        Reveal::Missing
    );
    assert_eq!(
        reveal_target(
            &nodes,
            &CollapseState::default(),
            "",
            &HashSet::new(),
            Some("other"),
            "users"
        ),
        Reveal::Missing,
        "the schema is part of the identity"
    );
}

#[test]
fn identically_named_tables_are_revealed_by_schema_identity() {
    let nodes = build_nodes(&[info(Some("archive"), "users"), info(Some("public"), "users")], &[]);
    let plan = reveal_target(
        &nodes,
        &CollapseState::default(),
        "",
        &HashSet::new(),
        Some("public"),
        "users",
    );
    let Reveal::Show { index, .. } = plan else {
        panic!("{plan:?}");
    };
    assert!(matches!(
        &nodes[index].kind,
        NodeKind::Table { schema: Some(schema), name } if schema == "public" && name == "users"
    ));
}

#[test]
fn a_view_and_an_unqualified_table_are_found_too() {
    let nodes = two_schemas();
    let view = reveal_target(
        &nodes,
        &CollapseState::default(),
        "",
        &HashSet::new(),
        Some("public"),
        "recent",
    );
    assert!(
        matches!(view, Reveal::Show { index, .. } if label(&nodes[index]) == "recent"),
        "{view:?}"
    );

    let single = build_nodes(&[info(None, "orders")], &[]);
    let plan = reveal_target(&single, &CollapseState::default(), "", &HashSet::new(), None, "orders");
    assert!(
        matches!(plan, Reveal::Show { index, .. } if label(&single[index]) == "orders"),
        "{plan:?}"
    );
}
