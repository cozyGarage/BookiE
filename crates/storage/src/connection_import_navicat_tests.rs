use super::*;

const SENTINEL: &str = "not-a-real-secret-7a2e";

fn export() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Connections Ver="1.4">
  <Connection ConnectionName="shop &amp; co" ProjectUUID="" ConnType="MYSQL" Host="DB.Example.com" Port="3307"
    Database="shop" UserName="bob" Password="{SENTINEL}" SavePassword="true" SSL="true" SSH="false" SSH_Password="{SENTINEL}"/>
  <Connection ConnectionName="pg" ConnType="POSTGRESQL" Host="10.0.0.5" Port="" Database="app" UserName="alice" SSL="false" SSH="false"/>
  <Connection ConnectionName="tunnelled" ConnType="MYSQL" Host="10.0.0.9" Port="3306" SSH="true" SSH_Host="bastion"/>
  <Connection ConnectionName="oracle" ConnType="ORACLE" Host="10.0.0.7" Port="1521"/>
  <Connection ConnectionName="bad host" ConnType="MYSQL" Host="evil host;rm" Port="3306"/>
  <Connection ConnectionName="bad port" ConnType="MYSQL" Host="h" Port="99999"/>
</Connections>"#
    )
}

#[test]
fn connections_carry_engine_host_port_database_user_and_tls() {
    let import = parse_navicat_connections(&export()).unwrap();
    let names: Vec<_> = import.drafts.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["pg", "shop & co"]);
    let pg = &import.drafts[0];
    assert_eq!(
        (pg.driver_id.as_str(), pg.port, pg.username.as_str()),
        ("postgres", 5432, "alice")
    );
    assert_eq!(pg.database, "app");
    assert!(!pg.use_tls);
    let shop = &import.drafts[1];
    assert_eq!(
        (shop.driver_id.as_str(), shop.port, shop.host.as_str()),
        ("mysql", 3307, "db.example.com")
    );
    assert!(shop.use_tls);
    assert_eq!(shop.tls_mode, Some(TlsMode::VerifyFull));
}

#[test]
fn ssh_unsupported_engine_bad_host_and_bad_port_are_skipped_by_name() {
    let import = parse_navicat_connections(&export()).unwrap();
    assert_eq!(import.skipped, ["bad host", "bad port", "oracle", "tunnelled"]);
}

#[test]
fn no_password_reaches_the_drafts_or_the_import_body() {
    let import = parse_navicat_connections(&export()).unwrap();
    assert!(!format!("{:?}", import.drafts).contains(SENTINEL));
    assert!(!format!("{:?}", import.into_body()).contains(SENTINEL));
}

#[test]
fn malformed_or_wrongly_shaped_input_is_refused_without_echoing_it() {
    for bad in [
        "",
        "not xml at all",
        "<Connections/>",
        "<a><b></a>",
        "<project><Connection ConnType=\"MYSQL\"/></project>",
    ] {
        let error = parse_navicat_connections(bad).unwrap_err().to_string();
        assert!(!error.contains("not xml at all"), "{error}");
    }
}

#[test]
fn a_file_where_no_engine_is_supported_is_refused() {
    let xml = r#"<Connections><Connection ConnectionName="o" ConnType="ORACLE" Host="h"/></Connections>"#;
    assert!(parse_navicat_connections(xml).is_err());
}

#[test]
fn input_over_the_size_limit_is_refused() {
    let big = format!("<Connections>{}</Connections>", " ".repeat(MAX_NAVICAT_BYTES));
    assert!(parse_navicat_connections(&big).is_err());
}

#[test]
fn a_custom_entity_is_refused_and_never_expanded() {
    let xml = r#"<!DOCTYPE p [<!ENTITY x SYSTEM "file:///etc/passwd">]><Connections>
        <Connection ConnectionName="a&x;" ConnType="MYSQL" Host="h" Port="3306"/></Connections>"#;
    assert!(parse_navicat_connections(xml).is_err());
}
