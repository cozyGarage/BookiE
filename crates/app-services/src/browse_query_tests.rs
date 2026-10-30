use super::*;
use tablepro_core::{FilterOp, FilterRule, FilterValue};
#[path = "browse_query/tests/reverse_paging.rs"]
mod reverse_paging;
#[path = "browse_query/tests/stable_order.rs"]
mod stable_order;
fn column(name: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "text".into(),
        nullable: false,
        primary_key,
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
fn default_order_uses_every_primary_key_column() {
    let columns = vec![column("tenant", true), column("name", false), column("id", true)];
    assert_eq!(
        resolved_order_by("postgres", &columns, None, false).as_deref(),
        Some("\"tenant\" ASC, \"id\" ASC")
    );
}

#[test]
fn explicit_sort_appends_primary_key_tie_breakers() {
    let columns = vec![column("tenant", true), column("name", false), column("id", true)];
    assert_eq!(
        resolved_order_by("postgres", &columns, Some((1, false)), false).as_deref(),
        Some("\"name\" DESC, \"tenant\" ASC, \"id\" ASC")
    );
}

#[test]
fn sorted_primary_key_is_not_duplicated() {
    let columns = vec![column("tenant", true), column("id", true)];
    assert_eq!(
        resolved_order_by("postgres", &columns, Some((0, false)), false).as_deref(),
        Some("\"tenant\" DESC, \"id\" ASC")
    );
}

#[test]
fn hidden_filter_column_stays_in_predicate_but_not_projection() {
    let columns = vec![
        column("tenant", true),
        column("name", false),
        column("status", false),
        column("secret", false),
    ];
    let filter = FilterSet {
        rules: vec![FilterRule {
            column: "status".into(),
            op: FilterOp::Eq,
            value: Some(FilterValue::Single("ready".into())),
        }],
        ..Default::default()
    };
    let hidden: HashSet<String> = ["status", "secret"].map(str::to_owned).into();
    let target = BrowseTarget {
        driver_id: "postgres",
        schema: None,
        table: "items",
        columns: &columns,
        filter: &filter,
        hidden_columns: Some(&hidden),
    };

    let PageQuery::Sql(page) = target.page(0, 100, None, None).unwrap() else {
        panic!("expected projected SQL")
    };
    assert_eq!(
        page.sql,
        "SELECT \"tenant\", \"name\" FROM \"items\" WHERE \"status\" = $1 ORDER BY \"tenant\" ASC LIMIT 100 OFFSET 0"
    );
    assert_eq!(page.projected_columns, Some(vec![0, 1]));
    assert_eq!(page.params, vec![Value::Text("ready".into())]);
}

#[test]
fn hidden_sort_column_stays_in_order_clause_and_composite_keys_stay_projected() {
    let columns = vec![
        column("tenant", true),
        column("id", true),
        column("name", false),
        column("rank", false),
        column("secret", false),
    ];
    let hidden: HashSet<String> = ["tenant", "rank", "secret"].map(str::to_owned).into();
    let target = BrowseTarget {
        driver_id: "postgres",
        schema: None,
        table: "items",
        columns: &columns,
        filter: &FilterSet::default(),
        hidden_columns: Some(&hidden),
    };

    let PageQuery::Sql(page) = target.page(0, 100, Some((3, true)), None).unwrap() else {
        panic!("expected projected SQL")
    };
    assert!(page.sql.starts_with("SELECT \"tenant\", \"id\", \"name\" FROM"));
    assert_eq!(page.projected_columns, Some(vec![0, 1, 2]));
    assert!(
        page.sql
            .ends_with("ORDER BY \"rank\" ASC, \"tenant\" ASC, \"id\" ASC LIMIT 100 OFFSET 0")
    );
}

#[test]
fn hidden_projection_falls_back_without_a_key() {
    let columns = vec![column("name", false), column("secret", false)];
    let hidden: HashSet<String> = ["secret"].map(str::to_owned).into();
    let target = BrowseTarget {
        driver_id: "postgres",
        schema: None,
        table: "items",
        columns: &columns,
        filter: &FilterSet::default(),
        hidden_columns: Some(&hidden),
    };
    assert_eq!(target.projected_columns(), None);
}

#[test]
fn hidden_projection_stays_disabled_when_no_non_key_column_is_hidden() {
    let columns = vec![column("id", true), column("name", false)];
    for hidden in [["id"], ["stale"]] {
        let hidden: HashSet<String> = hidden.map(str::to_owned).into();
        let target = BrowseTarget {
            driver_id: "postgres",
            schema: None,
            table: "items",
            columns: &columns,
            filter: &FilterSet::default(),
            hidden_columns: Some(&hidden),
        };
        assert_eq!(target.projected_columns(), None);
    }
}

#[test]
fn hidden_projection_stays_disabled_for_native_document_and_key_browsing() {
    let columns = vec![column("_id", true), column("name", false), column("secret", false)];
    let hidden: HashSet<String> = ["secret"].map(str::to_owned).into();
    for driver_id in ["mongodb", "redis"] {
        let target = BrowseTarget {
            driver_id,
            schema: None,
            table: "items",
            columns: &columns,
            filter: &FilterSet::default(),
            hidden_columns: Some(&hidden),
        };
        assert_eq!(target.projected_columns(), None);
        assert!(matches!(target.page(0, 100, None, None).unwrap(), PageQuery::Native));
    }
}

#[test]
fn hidden_raw_filter_column_stays_in_sql_but_not_projection() {
    let columns = vec![column("id", true), column("name", false), column("secret", false)];
    let filter = FilterSet {
        extra_sql: Some("secret <> ''".into()),
        ..Default::default()
    };
    let hidden: HashSet<String> = ["secret"].map(str::to_owned).into();
    let target = BrowseTarget {
        driver_id: "postgres",
        schema: None,
        table: "items",
        columns: &columns,
        filter: &filter,
        hidden_columns: Some(&hidden),
    };

    let PageQuery::Sql(page) = target.page(0, 100, None, None).unwrap() else {
        panic!("expected projected SQL")
    };
    assert!(page.sql.starts_with("SELECT \"id\", \"name\" FROM"));
    assert!(page.sql.contains("secret <> ''"));
    assert_eq!(page.projected_columns, Some(vec![0, 1]));
}

#[test]
fn value_query_fetches_one_column_by_primary_key() {
    let columns = vec![column("tenant", true), column("id", true), column("payload", false)];
    let target = BrowseTarget {
        driver_id: "postgres",
        schema: Some("app"),
        table: "records",
        columns: &columns,
        filter: &FilterSet::default(),
        hidden_columns: None,
    };
    let query = target
        .value_query(2, &[Value::Text("acme".into()), Value::Int(7)])
        .unwrap();

    assert_eq!(
        query.sql,
        "SELECT \"payload\" FROM \"app\".\"records\" WHERE \"tenant\" = $1 AND \"id\" = $2 ORDER BY \"tenant\" ASC, \"id\" ASC LIMIT 2 OFFSET 0"
    );
    assert_eq!(query.params, vec![Value::Text("acme".into()), Value::Int(7)]);
}

#[test]
fn unfiltered_unsorted_non_sql_fetch_stays_native_with_a_primary_key() {
    let columns = vec![column("_id", true)];
    for driver_id in ["mongodb", "redis"] {
        let target = BrowseTarget {
            driver_id,
            schema: None,
            table: "stats",
            columns: &columns,
            filter: &FilterSet::default(),
            hidden_columns: None,
        };
        assert!(matches!(target.page(0, 100, None, None).unwrap(), PageQuery::Native));
    }
}
