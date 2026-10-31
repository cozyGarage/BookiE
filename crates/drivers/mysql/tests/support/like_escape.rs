#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{connect, start_mysql};
use tablepro_core::{ColumnInfo, FilterOp, FilterRule, FilterSet, FilterValue, Value, build_filter_where};

fn text_column() -> ColumnInfo {
    ColumnInfo {
        name: "v".into(),
        data_type: "varchar(40)".into(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

#[tokio::test]
#[ignore = "requires docker"]
async fn contains_matches_literal_wildcards_with_and_without_no_backslash_escapes() {
    let (_container, options) = start_mysql().await;
    let setup = connect(options.clone()).await;
    setup.execute("CREATE TABLE t (v VARCHAR(40))").await.unwrap();
    setup
        .execute("INSERT INTO t VALUES ('50% off'), ('500 off'), ('a_b'), ('aXb'), ('pipe|x')")
        .await
        .unwrap();
    setup
        .execute("SET GLOBAL sql_mode = 'NO_BACKSLASH_ESCAPES'")
        .await
        .unwrap();
    setup.close().await.unwrap();
    let connection = connect(options).await;
    let column = text_column();

    for (needle, expected) in [("50%", "50% off"), ("a_b", "a_b"), ("e|x", "pipe|x")] {
        let set = FilterSet {
            rules: vec![FilterRule {
                column: "v".into(),
                op: FilterOp::Contains,
                value: Some(FilterValue::Single(needle.into())),
            }],
            ..Default::default()
        };
        let (clause, params) = build_filter_where("mysql", std::slice::from_ref(&column), &set)
            .unwrap()
            .unwrap();

        let rows = connection
            .query_params(&format!("SELECT v FROM t WHERE {clause}"), &params)
            .await
            .unwrap();

        assert_eq!(rows.rows, vec![vec![Value::Text(expected.into())]], "{needle}");
    }
}
