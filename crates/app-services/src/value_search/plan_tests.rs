use bookie_core::{ColumnInfo, SearchMode, Value};

use super::*;

fn column(name: &str, data_type: &str, primary_key: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        nullable: true,
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

fn people(schema: Option<&str>) -> SearchTable {
    SearchTable {
        target: TableTarget {
            schema: schema.map(str::to_owned),
            name: "people".into(),
        },
        columns: vec![
            column("id", "integer", true),
            column("name", "text", false),
            column("bio", "text", false),
            column("score", "integer", false),
            column("avatar", "bytea", false),
        ],
    }
}

fn contains() -> SearchOptions {
    SearchOptions {
        mode: SearchMode::Contains,
        include_non_text: false,
    }
}

#[test]
fn a_postgres_search_binds_the_term_once_per_condition_and_limits_the_rows() {
    let plan = plan_table("postgres", &people(Some("public")), "ada", contains())
        .expect("plan")
        .expect("searchable");
    assert_eq!(
        plan.statement,
        "SELECT \"id\", CASE WHEN \"name\" ILIKE $1 THEN 1 ELSE 0 END AS \"__bookie_hit_0\", \
         CASE WHEN \"bio\" ILIKE $2 THEN 1 ELSE 0 END AS \"__bookie_hit_1\" \
         FROM \"public\".\"people\" WHERE \"name\" ILIKE $3 OR \"bio\" ILIKE $4 LIMIT 26 OFFSET 0"
    );
    assert_eq!(plan.params, vec![Value::Text("%ada%".into()); 4]);
    assert_eq!(plan.searched, ["name", "bio"]);
    assert_eq!(plan.display, ["id"]);
}

#[test]
fn each_engine_gets_its_own_quoting_placeholders_and_row_window() {
    let mysql = plan_table("mysql", &people(None), "ada", contains())
        .expect("plan")
        .expect("plan");
    assert!(mysql.statement.contains("`name` LIKE ? ESCAPE '|'"));
    assert!(mysql.statement.ends_with("LIMIT 26 OFFSET 0"));
    let sqlite = plan_table("sqlite", &people(None), "ada", contains())
        .expect("plan")
        .expect("plan");
    assert!(sqlite.statement.contains("\"name\" LIKE ? ESCAPE '\\'"));
    let duckdb = plan_table("duckdb", &people(None), "ada", contains())
        .expect("plan")
        .expect("plan");
    assert!(duckdb.statement.contains("\"name\" ILIKE ? ESCAPE '\\'"));
    let mssql = plan_table("mssql", &people(Some("dbo")), "ada", contains())
        .expect("plan")
        .expect("plan");
    assert!(mssql.statement.contains("[dbo].[people]"));
    assert!(mssql.statement.contains("[name] LIKE @P1 ESCAPE '\\'"));
    assert!(mssql.statement.ends_with("OFFSET 0 ROWS FETCH NEXT 26 ROWS ONLY"));
}

#[test]
fn a_hostile_term_is_bound_and_never_part_of_the_statement() {
    let hostile = "x'; DROP TABLE people; -- 100%_";
    let plan = plan_table("postgres", &people(None), hostile, contains())
        .expect("plan")
        .expect("plan");
    assert!(!plan.statement.contains("DROP"));
    assert!(!plan.statement.contains(hostile));
    assert_eq!(
        plan.params[0],
        Value::Text("%x'; DROP TABLE people; -- 100\\%\\_%".into())
    );
}

#[test]
fn hostile_identifiers_are_quoted_or_refused() {
    let mut table = people(None);
    table.columns[1].name = "na\"me; DROP TABLE people".into();
    table.target.name = "pe]ople`".into();
    let plan = plan_table("postgres", &table, "ada", contains())
        .expect("plan")
        .expect("plan");
    assert!(plan.statement.contains("\"na\"\"me; DROP TABLE people\""));
    let mysql = plan_table("mysql", &table, "ada", contains())
        .expect("plan")
        .expect("plan");
    assert!(mysql.statement.contains("`pe]ople```"));

    let mut control = people(None);
    control.columns[1].name = "bad\u{0}name".into();
    assert!(matches!(
        plan_table("postgres", &control, "ada", contains()),
        Err(PlanError::Ident(_))
    ));
    let mut blank = people(None);
    blank.target.name = "  ".into();
    assert!(matches!(
        plan_table("postgres", &blank, "ada", contains()),
        Err(PlanError::Ident(_))
    ));
}

#[test]
fn the_term_must_be_present_and_bounded() {
    assert_eq!(validate_term("   "), Err(PlanError::EmptyTerm));
    assert_eq!(validate_term(""), Err(PlanError::EmptyTerm));
    let long = "a".repeat(MAX_TERM_CHARS + 1);
    assert_eq!(
        validate_term(&long),
        Err(PlanError::TermTooLong { max: MAX_TERM_CHARS })
    );
    assert!(validate_term(&"a".repeat(MAX_TERM_CHARS)).is_ok());
    assert!(plan_table("postgres", &people(None), "", contains()).is_err());
}

#[test]
fn a_table_without_a_searchable_column_has_no_plan() {
    let table = SearchTable {
        target: TableTarget {
            schema: None,
            name: "blobs".into(),
        },
        columns: vec![column("id", "integer", true), column("data", "bytea", false)],
    };
    assert_eq!(plan_table("postgres", &table, "ada", contains()), Ok(None));
}

#[test]
fn numeric_and_uuid_columns_are_searched_only_for_an_exact_match_that_asks_for_them() {
    let exact = SearchOptions {
        mode: SearchMode::Exact,
        include_non_text: false,
    };
    let text_only = plan_table("postgres", &people(None), "42", exact)
        .expect("plan")
        .expect("plan");
    assert_eq!(text_only.searched, ["name", "bio"]);
    let everything = SearchOptions {
        include_non_text: true,
        ..exact
    };
    let plan = plan_table("postgres", &people(None), "42", everything)
        .expect("plan")
        .expect("plan");
    assert_eq!(plan.searched, ["id", "name", "bio", "score"]);
    assert!(plan.params.contains(&Value::Int(42)));
    assert!(plan.params.contains(&Value::Text("42".into())));
}

#[test]
fn a_table_with_many_columns_searches_a_bounded_number_and_counts_the_rest() {
    let columns: Vec<ColumnInfo> = (0..40)
        .map(|index| column(&format!("c{index}"), "text", index == 0))
        .collect();
    let table = SearchTable {
        target: TableTarget {
            schema: None,
            name: "wide".into(),
        },
        columns,
    };
    let plan = plan_table("postgres", &table, "ada", contains())
        .expect("plan")
        .expect("plan");
    assert_eq!(plan.searched.len(), MAX_SEARCH_COLUMNS_PER_TABLE);
    assert_eq!(plan.columns_not_searched, 40 - MAX_SEARCH_COLUMNS_PER_TABLE);
    assert_eq!(plan.params.len(), MAX_SEARCH_COLUMNS_PER_TABLE * 2);
}

#[test]
fn without_a_primary_key_the_first_plain_columns_are_shown() {
    let table = SearchTable {
        target: TableTarget {
            schema: None,
            name: "logs".into(),
        },
        columns: vec![
            column("payload", "jsonb", false),
            column("level", "text", false),
            column("message", "text", false),
            column("at", "timestamp", false),
            column("extra", "text", false),
        ],
    };
    let plan = plan_table("postgres", &table, "boom", contains())
        .expect("plan")
        .expect("plan");
    assert_eq!(plan.display, ["level", "message", "at"]);
}

#[test]
fn opening_a_hit_filters_on_the_matched_columns_with_the_right_operator() {
    let outcome = TableOutcome {
        target: TableTarget {
            schema: None,
            name: "people".into(),
        },
        status: TableStatus::Searched,
        hits: Vec::new(),
        matched_columns: vec!["name".into(), "bio".into()],
        more_rows_exist: false,
        columns_not_searched: 0,
    };
    let plain = open_filter(&outcome, "ada", SearchMode::Contains);
    assert_eq!(plain.combinator, bookie_core::Combinator::Or);
    assert_eq!(plain.rules.len(), 2);
    assert_eq!(plain.rules[0].op, bookie_core::FilterOp::Ilike);
    assert_eq!(
        plain.rules[0].value,
        Some(bookie_core::FilterValue::Single("%ada%".into()))
    );
    let wildcard = open_filter(&outcome, "50%", SearchMode::Contains);
    assert_eq!(wildcard.rules[0].op, bookie_core::FilterOp::Contains);
    assert_eq!(
        wildcard.rules[0].value,
        Some(bookie_core::FilterValue::Single("50%".into()))
    );
    let exact = open_filter(&outcome, "ada", SearchMode::Exact);
    assert_eq!(exact.rules[0].op, bookie_core::FilterOp::Eq);
}
