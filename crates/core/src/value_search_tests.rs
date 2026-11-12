use super::*;

fn column(name: &str, data_type: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
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

fn clause(driver: &str, data_type: &str, term: &str, mode: SearchMode, all: bool) -> Option<SearchClause> {
    search_clause(driver, &column("name", data_type), term, mode, all, 0)
}

#[test]
fn contains_uses_the_native_case_insensitive_operator_where_the_engine_has_one() {
    let postgres = clause("postgres", "text", "ada", SearchMode::Contains, false).expect("clause");
    assert_eq!(postgres.sql, "\"name\" ILIKE $1");
    assert_eq!(postgres.value, Value::Text("%ada%".into()));
    let duckdb = clause("duckdb", "VARCHAR", "ada", SearchMode::Contains, false).expect("clause");
    assert_eq!(duckdb.sql, "\"name\" ILIKE ? ESCAPE '\\'");
}

#[test]
fn contains_declares_the_escape_character_each_engine_needs() {
    let mysql = clause("mysql", "varchar(20)", "ada", SearchMode::Contains, false).expect("clause");
    assert_eq!(mysql.sql, "`name` LIKE ? ESCAPE '|'");
    let sqlite = clause("sqlite", "TEXT", "ada", SearchMode::Contains, false).expect("clause");
    assert_eq!(sqlite.sql, "\"name\" LIKE ? ESCAPE '\\'");
    let mssql = clause("mssql", "nvarchar", "ada", SearchMode::Contains, false).expect("clause");
    assert_eq!(mssql.sql, "[name] LIKE ? ESCAPE '\\'".replace('?', "@P1"));
}

#[test]
fn wildcards_in_the_term_are_escaped_and_the_term_is_bound() {
    let hostile = "50%_off\\' OR 1=1 --";
    let postgres = clause("postgres", "text", hostile, SearchMode::Contains, false).expect("clause");
    assert!(!postgres.sql.contains("OR 1=1"));
    assert_eq!(postgres.value, Value::Text("%50\\%\\_off\\\\' OR 1=1 --%".into()));
    let mysql = clause("mysql", "text", "a%b_c|d", SearchMode::Contains, false).expect("clause");
    assert_eq!(mysql.value, Value::Text("%a|%b|_c||d%".into()));
    let mssql = clause("mssql", "nvarchar", "[x]", SearchMode::Contains, false).expect("clause");
    assert_eq!(mssql.value, Value::Text("%\\[x]%".into()));
}

#[test]
fn postgres_casts_non_text_pattern_columns_but_not_plain_text() {
    let enum_like = clause("postgres", "citext", "ada", SearchMode::Contains, false);
    assert!(enum_like.is_none() || enum_like.is_some_and(|clause| clause.sql.contains("CAST")));
    let varchar = clause("postgres", "character varying", "ada", SearchMode::Contains, false).expect("clause");
    assert!(!varchar.sql.contains("CAST"));
}

#[test]
fn contains_skips_columns_that_are_not_text() {
    for data_type in [
        "integer",
        "uuid",
        "numeric(10,2)",
        "timestamp",
        "bytea",
        "jsonb",
        "boolean",
    ] {
        assert!(
            clause("postgres", data_type, "1", SearchMode::Contains, true).is_none(),
            "{data_type}"
        );
    }
}

#[test]
fn exact_matches_text_by_default_and_numbers_only_when_asked() {
    let text = clause("postgres", "text", "ada", SearchMode::Exact, false).expect("clause");
    assert_eq!(text.value, Value::Text("ada".into()));
    assert!(text.sql.starts_with("\"name\" = $1"));
    assert!(clause("postgres", "integer", "42", SearchMode::Exact, false).is_none());
    let number = clause("postgres", "integer", "42", SearchMode::Exact, true).expect("clause");
    assert_eq!(number.value, Value::Int(42));
    let uuid = clause(
        "postgres",
        "uuid",
        "6f1c7a62-3d1b-4c63-9a1a-0e5f3f6a8b11",
        SearchMode::Exact,
        true,
    )
    .expect("clause");
    assert!(matches!(uuid.value, Value::Uuid(_)));
}

#[test]
fn exact_skips_a_numeric_column_when_the_term_is_not_a_number() {
    assert!(clause("postgres", "integer", "forty-two", SearchMode::Exact, true).is_none());
    assert!(clause("mysql", "decimal(10,2)", "1.2.3", SearchMode::Exact, true).is_none());
}

#[test]
fn json_and_binary_columns_are_never_searched() {
    for mode in [SearchMode::Contains, SearchMode::Exact] {
        for data_type in ["jsonb", "json", "bytea", "blob"] {
            assert!(clause("postgres", data_type, "x", mode, true).is_none(), "{data_type}");
        }
    }
}

#[test]
fn placeholder_numbering_follows_the_index_given() {
    let third = search_clause(
        "postgres",
        &column("name", "text"),
        "ada",
        SearchMode::Contains,
        false,
        2,
    )
    .expect("clause");
    assert!(third.sql.ends_with("$3"));
}
