use super::*;

const SENTINEL: &str = "not-a-real-secret-91ce";

fn main_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project version="4">
  <component name="DataSourceManagerImpl" format="xml" multifile-model="true">
    <data-source source="LOCAL" name="orders &amp; co" uuid="u-pg">
      <driver-ref>postgresql</driver-ref>
      <jdbc-driver>org.postgresql.Driver</jdbc-driver>
      <jdbc-url>jdbc:postgresql://DB.Example.com:5433/orders?user=bob&amp;password={SENTINEL}</jdbc-url>
    </data-source>
    <data-source source="LOCAL" name="shop" uuid="u-my">
      <jdbc-url>jdbc:mysql://10.0.0.5/shop</jdbc-url>
    </data-source>
    <data-source source="LOCAL" name="mssql" uuid="u-ms">
      <jdbc-url>jdbc:sqlserver://sql.example.com:1434;databaseName=sales;user=sa;password={SENTINEL}</jdbc-url>
    </data-source>
    <data-source source="LOCAL" name="tunnelled" uuid="u-ssh">
      <jdbc-url>jdbc:postgresql://10.0.0.9:5432/app</jdbc-url>
    </data-source>
    <data-source source="LOCAL" name="cluster" uuid="u-multi">
      <jdbc-url>jdbc:postgresql://a:5432,b:5432/app</jdbc-url>
    </data-source>
    <data-source source="LOCAL" name="local file" uuid="u-sqlite">
      <jdbc-url>jdbc:sqlite:$PROJECT_DIR$/x.db</jdbc-url>
    </data-source>
    <data-source source="LOCAL" name="oracle" uuid="u-ora">
      <jdbc-url>jdbc:oracle:thin:@h:1521:XE</jdbc-url>
    </data-source>
    <data-source source="LOCAL" name="bad host" uuid="u-bad">
      <jdbc-url>jdbc:mysql://evil host;rm/db</jdbc-url>
    </data-source>
  </component>
</project>"#
    )
}

fn local_xml() -> String {
    format!(
        r#"<project version="4">
  <component name="dataSourceStorageLocal">
    <data-source name="orders &amp; co" uuid="u-pg">
      <secret-storage>master_key</secret-storage>
      <user-name>alice</user-name>
      <password>{SENTINEL}</password>
    </data-source>
    <data-source name="tunnelled" uuid="u-ssh">
      <user-name>carol</user-name>
      <ssh-properties>
        <enabled>true</enabled>
        <user>vagrant</user>
      </ssh-properties>
    </data-source>
  </component>
</project>"#
    )
}

#[test]
fn drafts_carry_engine_host_port_database_and_the_user_from_the_local_file() {
    let import = parse_datagrip_data_sources(&main_xml(), Some(&local_xml())).unwrap();
    let names: Vec<_> = import.drafts.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["mssql", "orders & co", "shop"]);
    let key = |i: usize| {
        let c = &import.drafts[i];
        (
            c.driver_id.clone(),
            c.host.clone(),
            c.port,
            c.database.clone(),
            c.username.clone(),
        )
    };
    assert_eq!(
        key(0),
        (
            "mssql".into(),
            "sql.example.com".into(),
            1434,
            "sales".into(),
            String::new()
        )
    );
    assert_eq!(
        key(1),
        (
            "postgres".into(),
            "db.example.com".into(),
            5433,
            "orders".into(),
            "alice".into()
        )
    );
    assert_eq!(
        key(2),
        ("mysql".into(), "10.0.0.5".into(), 3306, "shop".into(), String::new())
    );
}

#[test]
fn ssh_multi_host_unsupported_engine_and_bad_host_sources_are_skipped_by_name() {
    let import = parse_datagrip_data_sources(&main_xml(), Some(&local_xml())).unwrap();
    assert_eq!(
        import.skipped,
        ["bad host", "cluster", "local file", "oracle", "tunnelled"]
    );
}

#[test]
fn the_local_file_is_optional() {
    let import = parse_datagrip_data_sources(&main_xml(), None).unwrap();
    assert_eq!(
        import.drafts.len(),
        4,
        "without the local file the ssh source is not known to need a tunnel"
    );
    assert!(import.drafts.iter().all(|draft| draft.username.is_empty()));
}

#[test]
fn no_password_from_the_url_or_the_local_file_reaches_the_drafts_or_the_import_body() {
    let import = parse_datagrip_data_sources(&main_xml(), Some(&local_xml())).unwrap();
    let body = import.into_body();
    assert!(body.secrets.is_empty());
    assert!(!serde_json::to_string(&body).unwrap().contains(SENTINEL));
}

#[test]
fn a_ssh_users_name_is_not_taken_for_the_database_user() {
    let local = r#"<project><component><data-source name="s" uuid="u-my">
        <ssh-properties><user>vagrant</user></ssh-properties></data-source></component></project>"#;
    let import = parse_datagrip_data_sources(&main_xml(), Some(local)).unwrap();
    assert!(import.drafts.iter().all(|draft| draft.username != "vagrant"));
}

#[test]
fn malformed_or_wrongly_shaped_input_is_refused_without_echoing_it() {
    for bad in [
        "",
        "not xml at all",
        "<project/>",
        "<a><b></a>",
        r#"<project><data-source name="x" uuid="y"/></project>"#,
    ] {
        let error = parse_datagrip_data_sources(bad, None).unwrap_err().to_string();
        assert!(!error.contains("not xml at all"), "{error}");
    }
    assert!(parse_datagrip_data_sources(&main_xml(), Some("<a><b></a>")).is_err());
}

#[test]
fn a_file_where_no_engine_is_supported_is_refused() {
    let xml = r#"<project><component><data-source name="o" uuid="u"><jdbc-url>jdbc:oracle:thin:@h:1521:XE</jdbc-url></data-source></component></project>"#;
    assert!(parse_datagrip_data_sources(xml, None).is_err());
}

#[test]
fn input_over_the_size_limit_is_refused() {
    let big = format!("<project>{}</project>", " ".repeat(MAX_DATAGRIP_BYTES));
    assert!(parse_datagrip_data_sources(&big, None).is_err());
}

#[test]
fn a_custom_entity_is_refused_and_never_expanded() {
    let xml = r#"<!DOCTYPE p [<!ENTITY x SYSTEM "file:///etc/passwd">]><project><component>
        <data-source name="a" uuid="u"><jdbc-url>jdbc:mysql://h/&x;</jdbc-url></data-source></component></project>"#;
    assert!(parse_datagrip_data_sources(xml, None).is_err());
}

fn grouped_xml(groups: &[(&str, &str)]) -> String {
    let sources: String = groups
        .iter()
        .map(|(name, group)| {
            format!(
                r#"    <data-source source="LOCAL" name="{name}" group="{group}" uuid="u-{name}">
      <jdbc-url>jdbc:postgresql://10.0.0.5:5432/{name}</jdbc-url>
    </data-source>
"#
            )
        })
        .collect();
    format!(r#"<project version="4"><component name="DataSourceManagerImpl">{sources}</component></project>"#)
}

fn group_of(import: &PeerImport, name: &str) -> Option<String> {
    let draft = import.drafts.iter().find(|draft| draft.name == name)?;
    import.groups.get(&draft.id).cloned()
}

#[test]
fn the_group_attribute_becomes_a_group_and_a_slash_flattens_nested_folders() {
    let xml = grouped_xml(&[("flat", "tests"), ("nested", "prod/eu/"), ("spaced", " a /  b ")]);
    let import = parse_datagrip_data_sources(&xml, None).unwrap();
    assert_eq!(group_of(&import, "flat").as_deref(), Some("tests"));
    assert_eq!(group_of(&import, "nested").as_deref(), Some("prod / eu"));
    assert_eq!(group_of(&import, "spaced").as_deref(), Some("a / b"));
    assert_eq!(import.into_body().organization.len(), 3);
}

#[test]
fn ungrouped_empty_over_long_or_control_group_names_import_without_a_group() {
    let long = "x".repeat(300);
    let xml = grouped_xml(&[("none", ""), ("slashes", "//"), ("long", &long), ("control", "a&#9;b")]);
    let import = parse_datagrip_data_sources(&xml, None).unwrap();
    assert_eq!(import.drafts.len(), 4);
    assert!(import.groups.is_empty(), "{:?}", import.groups);
}
