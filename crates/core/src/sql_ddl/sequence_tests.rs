use super::{BuildDdlError, build_restart_sequence};

#[test]
fn a_postgres_sequence_restarts_at_the_requested_value_with_quoted_names() {
    let sql = build_restart_sequence("postgres", Some("odd\"schema"), "ticket numbers", 42).unwrap();

    assert_eq!(
        sql,
        "ALTER SEQUENCE \"odd\"\"schema\".\"ticket numbers\" RESTART WITH 42"
    );
}

#[test]
fn a_negative_and_an_extreme_value_are_written_as_plain_integers() {
    assert!(
        build_restart_sequence("postgres", None, "s", i64::MIN)
            .unwrap()
            .ends_with("RESTART WITH -9223372036854775808")
    );
    assert!(
        build_restart_sequence("postgres", None, "s", i64::MAX)
            .unwrap()
            .ends_with("RESTART WITH 9223372036854775807")
    );
}

#[test]
fn an_empty_sequence_name_is_refused() {
    assert!(matches!(
        build_restart_sequence("postgres", None, "  ", 1),
        Err(BuildDdlError::EmptyTableName)
    ));
}

#[test]
fn engines_without_a_restart_statement_here_are_refused() {
    for driver in ["mysql", "sqlite", "mssql", "oracle"] {
        assert!(
            matches!(
                build_restart_sequence(driver, None, "s", 1),
                Err(BuildDdlError::UnsupportedDriver(_))
            ),
            "{driver}"
        );
    }
}
