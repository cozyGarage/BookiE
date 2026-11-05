use super::*;

const SENTINEL: &str = "not-a-real-secret-0b7f";

fn sources() -> String {
    format!(
        r#"{{
  "folders": {{"Prod": {{}}}},
  "connections": {{
    "pg-1": {{
      "provider": "postgresql", "driver": "postgres-jdbc", "name": "Orders PG", "folder": "Prod",
      "save-password": true,
      "configuration": {{
        "host": "DB.Example.com", "port": "5433", "database": "orders", "user": "alice",
        "url": "jdbc:postgresql://DB.Example.com:5433/orders", "password": "{SENTINEL}"
      }}
    }},
    "my-1": {{
      "provider": "mysql", "driver": "mysql8", "name": "Shop",
      "configuration": {{"host": "10.0.0.5", "database": "shop", "url": "jdbc:mysql://10.0.0.5:3306/shop"}}
    }},
    "ora-1": {{
      "provider": "oracle", "driver": "oracle_thin", "name": "Legacy Oracle",
      "configuration": {{"host": "ora.example.com", "port": 1521, "url": "jdbc:oracle:thin:@ora.example.com:1521:XE"}}
    }}
  }}
}}"#
    )
}

#[test]
fn drafts_carry_host_port_user_and_engine_and_skip_unsupported_engines_by_name() {
    let import = parse_dbeaver_data_sources(&sources()).unwrap();
    let names: Vec<_> = import.drafts.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Orders PG", "Shop"]);
    let pg = &import.drafts[0];
    assert_eq!(
        (
            pg.driver_id.as_str(),
            pg.host.as_str(),
            pg.port,
            pg.database.as_str(),
            pg.username.as_str()
        ),
        ("postgres", "db.example.com", 5433, "orders", "alice")
    );
    let mysql = &import.drafts[1];
    assert_eq!(
        (mysql.driver_id.as_str(), mysql.port, mysql.username.as_str()),
        ("mysql", 3306, "")
    );
    assert_eq!(import.skipped, ["Legacy Oracle"]);
}

#[test]
fn no_password_in_the_source_reaches_the_drafts() {
    let import = parse_dbeaver_data_sources(&sources()).unwrap();
    assert!(!format!("{import:?}").contains(SENTINEL));
    assert!(!serde_json::to_string(&import.drafts).unwrap().contains(SENTINEL));
}

#[test]
fn each_draft_gets_its_own_id() {
    let import = parse_dbeaver_data_sources(&sources()).unwrap();
    assert_ne!(import.drafts[0].id, import.drafts[1].id);
}

#[test]
fn malformed_or_wrongly_shaped_input_is_refused_without_echoing_it() {
    for bad in [
        "",
        "not json",
        "[]",
        r#"{"connections": []}"#,
        r#"{"connections": {"a": 5}}"#,
    ] {
        let error = parse_dbeaver_data_sources(bad).unwrap_err().to_string();
        assert!(!error.contains("not json"), "{error}");
    }
}

#[test]
fn a_file_with_no_connections_is_refused() {
    assert!(parse_dbeaver_data_sources(r#"{"connections": {}}"#).is_err());
}

#[test]
fn a_connection_with_a_bad_host_or_port_is_skipped_not_imported() {
    let json = r#"{"connections": {
        "a": {"provider": "postgresql", "name": "bad host", "configuration": {"host": "evil host;rm", "port": "5432"}},
        "b": {"provider": "postgresql", "name": "bad port", "configuration": {"host": "h", "port": "99999"}},
        "c": {"provider": "postgresql", "name": "ok", "configuration": {"host": "h"}}
    }}"#;
    let import = parse_dbeaver_data_sources(json).unwrap();
    assert_eq!(import.drafts.len(), 1);
    assert_eq!(import.drafts[0].port, 5432);
    assert_eq!(import.skipped.len(), 2);
}

#[test]
fn input_over_the_size_limit_is_refused() {
    let big = format!(r#"{{"connections": {{}}, "pad": "{}"}}"#, "x".repeat(MAX_DBEAVER_BYTES));
    assert!(parse_dbeaver_data_sources(&big).is_err());
}

#[test]
fn a_file_where_no_engine_is_supported_is_refused() {
    let json = r#"{"connections": {"o": {"provider": "oracle", "name": "Oracle", "configuration": {"host": "h"}}}}"#;
    assert!(parse_dbeaver_data_sources(json).is_err());
}

#[test]
fn the_import_body_holds_every_draft_and_no_secrets() {
    let import = parse_dbeaver_data_sources(&sources()).unwrap();
    let ids: Vec<_> = import.drafts.iter().map(|draft| draft.id).collect();
    let body = import.into_body();
    assert_eq!(body.connections.iter().map(|c| c.id).collect::<Vec<_>>(), ids);
    assert!(body.secrets.is_empty());
    assert!(!serde_json::to_string(&body).unwrap().contains(SENTINEL));
}

const FOLDERS: &str = r#"{
  "folders": {"Prod": {}, "EU": {"parent": "Prod"}, "Loop": {"parent": "Loop"}},
  "connections": {
    "a": {"provider": "postgresql", "name": "Nested", "folder": "EU", "configuration": {"host": "h1"}},
    "b": {"provider": "postgresql", "name": "Top", "folder": "Prod", "configuration": {"host": "h2"}},
    "c": {"provider": "postgresql", "name": "Plain", "configuration": {"host": "h3"}},
    "d": {"provider": "postgresql", "name": "Cyclic", "folder": "Loop", "configuration": {"host": "h4"}}
  }
}"#;

fn group_of(import: &PeerImport, name: &str) -> Option<String> {
    let draft = import.drafts.iter().find(|draft| draft.name == name)?;
    import.groups.get(&draft.id).cloned()
}

#[test]
fn a_nested_folder_becomes_one_flattened_group() {
    let import = parse_dbeaver_data_sources(FOLDERS).unwrap();
    assert_eq!(group_of(&import, "Nested").as_deref(), Some("Prod / EU"));
    assert_eq!(group_of(&import, "Top").as_deref(), Some("Prod"));
    assert_eq!(group_of(&import, "Plain"), None);
}

#[test]
fn a_cyclic_folder_chain_is_bounded() {
    let import = parse_dbeaver_data_sources(FOLDERS).unwrap();
    let group = group_of(&import, "Cyclic").unwrap();
    assert!(group.split(" / ").count() <= MAX_FOLDER_DEPTH, "{group}");
}

#[test]
fn the_import_body_carries_the_groups_and_no_secrets() {
    let import = parse_dbeaver_data_sources(FOLDERS).unwrap();
    let nested = import.drafts.iter().find(|draft| draft.name == "Nested").unwrap().id;
    let plain = import.drafts.iter().find(|draft| draft.name == "Plain").unwrap().id;
    let body = import.into_body();
    assert_eq!(
        body.organization.get(&nested).and_then(|entry| entry.group.as_deref()),
        Some("Prod / EU")
    );
    assert!(!body.organization.contains_key(&plain));
    assert!(body.secrets.is_empty());
}

#[test]
fn a_folder_name_with_control_characters_leaves_the_connection_ungrouped_but_imported() {
    let json = "{\"folders\": {}, \"connections\": {\"a\": {\"provider\": \"postgresql\", \"name\": \"X\", \"folder\": \"bad\\u0007name\", \"configuration\": {\"host\": \"h\"}}}}";
    let import = parse_dbeaver_data_sources(json).unwrap();
    assert_eq!(import.drafts.len(), 1);
    let body = import.into_body();
    assert!(
        body.organization.values().all(|entry| entry.group.is_none()),
        "{:?}",
        body.organization
    );
}

fn json_with_folder(folders: &str, folder: &str) -> String {
    format!(
        r#"{{"folders": {folders}, "connections": {{"a": {{"provider": "postgresql", "name": "X", "folder": "{folder}", "configuration": {{"host": "h"}}}}}}}}"#
    )
}

fn group_for(folders: &str, folder: &str) -> Option<String> {
    let import = parse_dbeaver_data_sources(&json_with_folder(folders, folder)).unwrap();
    group_of(&import, "X")
}

#[test]
fn a_documented_slash_path_becomes_one_flattened_group() {
    assert_eq!(group_for("{}", "prod/finance").as_deref(), Some("prod / finance"));
    assert_eq!(group_for("{}", "prod").as_deref(), Some("prod"));
    assert_eq!(group_for("{}", "a/b/c").as_deref(), Some("a / b / c"));
}

#[test]
fn empty_and_padded_path_segments_are_dropped() {
    assert_eq!(group_for("{}", " prod // finance/ ").as_deref(), Some("prod / finance"));
    assert_eq!(group_for("{}", "/").as_deref(), None);
    assert_eq!(group_for("{}", "").as_deref(), None);
}

#[test]
fn a_parent_listed_for_a_full_path_does_not_repeat_the_prefix() {
    let folders = r#"{"prod/finance": {"parent": "prod"}, "prod": {}}"#;
    assert_eq!(group_for(folders, "prod/finance").as_deref(), Some("prod / finance"));
}

#[test]
fn a_parent_listed_for_a_leaf_name_is_prepended() {
    let folders = r#"{"EU": {"parent": "Prod"}, "Prod": {}}"#;
    assert_eq!(group_for(folders, "EU").as_deref(), Some("Prod / EU"));
}

#[test]
fn a_two_folder_parent_cycle_ends_without_repeating_a_name() {
    let folders = r#"{"A": {"parent": "B"}, "B": {"parent": "A"}}"#;
    assert_eq!(group_for(folders, "A").as_deref(), Some("B / A"));
}

#[test]
fn a_folder_path_deeper_than_the_limit_keeps_the_innermost_levels() {
    let path = (1..=12).map(|level| format!("s{level}")).collect::<Vec<_>>().join("/");
    let group = group_for("{}", &path).unwrap();
    let levels: Vec<&str> = group.split(" / ").collect();
    assert_eq!(levels.len(), MAX_FOLDER_DEPTH, "{group}");
    assert_eq!((levels[0], levels[levels.len() - 1]), ("s5", "s12"));
}

#[test]
fn an_unexpected_folders_shape_never_refuses_the_connections() {
    for folders in [
        "[]",
        "\"text\"",
        "null",
        "5",
        "{\"a\": true}",
        "{\"a\": 5}",
        "{\"a\": {\"parent\": null}}",
    ] {
        let import = parse_dbeaver_data_sources(&json_with_folder(folders, "a"))
            .unwrap_or_else(|error| panic!("folders {folders} refused the file: {error}"));
        assert_eq!(import.drafts.len(), 1, "{folders}");
    }
}
