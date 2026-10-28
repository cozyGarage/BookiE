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

fn sheet(rows: &[&[&str]], headers: &[&str]) -> CsvSheet {
    CsvSheet {
        headers: headers.iter().map(|header| (*header).to_owned()).collect(),
        rows: rows
            .iter()
            .map(|row| row.iter().map(|cell| (*cell).to_owned()).collect())
            .collect(),
        truncated: false,
    }
}

fn target<'a>(columns: &'a [ColumnInfo], mapping: &'a [Option<usize>]) -> ImportTarget<'a> {
    ImportTarget {
        driver_id: "postgres",
        capabilities: DriverCapabilities {
            supports_explicit_auto_increment_values: true,
            ..DriverCapabilities::default()
        },
        schema: None,
        table: "people",
        columns,
        mapping,
    }
}

#[test]
fn every_row_binds_to_the_same_statement() {
    let columns = vec![column("id", "bigint"), column("name", "text")];
    let mapping = vec![Some(0), Some(1)];
    let sheet = sheet(&[&["1", "ada"], &["2", "grace"]], &["id", "name"]);

    let plan = build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect("plan");

    assert_eq!(
        plan.statement,
        "INSERT INTO \"people\" (\"id\", \"name\") VALUES ($1, $2)"
    );
    assert_eq!(plan.row_count(), 2);
    assert_eq!(plan.rows.len(), 2);
    assert_eq!(plan.rows[0], vec![Value::Int(1), Value::Text("ada".to_owned())]);
}

#[test]
fn mysql_empty_enum_set_and_varchar_csv_cells_are_reconstructed_without_binding_empty_text() {
    let columns = vec![
        column("id", "INT"),
        column("state", "enum('', 'ready', 'NULL')"),
        column("permissions", "set('read', 'write')"),
        column("note", "varchar(30)"),
    ];
    let mapping = vec![Some(0), Some(1), Some(2), Some(3)];
    let sheet = sheet(
        &[
            &["1", "", "", ""],
            &["2", "ready", "read", "plain"],
            &["3", "\\N", "\\N", "\\N"],
        ],
        &["id", "state", "permissions", "note"],
    );
    let options = CsvImportOptions {
        null_marker: "\\N".into(),
        ..CsvImportOptions::default()
    };
    let target = ImportTarget {
        driver_id: "mysql",
        capabilities: DriverCapabilities {
            supports_explicit_auto_increment_values: true,
            ..DriverCapabilities::default()
        },
        schema: None,
        table: "enum_rows",
        columns: &columns,
        mapping: &mapping,
    };

    let plan = build_insert_plan(&target, &sheet, &options).expect("plan");

    assert_eq!(
        plan.statement,
        "INSERT INTO `enum_rows` (`id`, `state`, `permissions`, `note`) VALUES (?, CASE WHEN ? = 1 THEN SPACE(0) ELSE ? END, CASE WHEN ? = 1 THEN SPACE(0) ELSE ? END, CASE WHEN ? = 1 THEN SPACE(0) ELSE ? END)"
    );
    assert_eq!(
        plan.rows,
        vec![
            vec![
                Value::Int(1),
                Value::Int(1),
                Value::Null,
                Value::Int(1),
                Value::Null,
                Value::Int(1),
                Value::Null,
            ],
            vec![
                Value::Int(2),
                Value::Int(0),
                Value::Text("ready".into()),
                Value::Int(0),
                Value::Text("read".into()),
                Value::Int(0),
                Value::Text("plain".into()),
            ],
            vec![
                Value::Int(3),
                Value::Int(0),
                Value::Null,
                Value::Int(0),
                Value::Null,
                Value::Int(0),
                Value::Null,
            ],
        ]
    );
}

#[test]
fn mysql_empty_string_reconstruction_is_limited_to_text_destinations() {
    for data_type in [
        "enum('', 'ready')",
        "set('read', 'write')",
        "char(8)",
        "varchar(30)",
        "tinytext",
        "text",
        "mediumtext",
        "longtext",
    ] {
        assert!(mysql_empty_string_type(data_type), "{data_type}");
    }
    for data_type in ["json", "varbinary", "int"] {
        assert!(!mysql_empty_string_type(data_type), "{data_type}");
    }
}

#[test]
fn mysql_text_import_without_empty_cells_keeps_the_simple_insert_plan() {
    let columns = vec![column("note", "text")];
    let mapping = vec![Some(0)];
    let sheet = sheet(&[&["plain"]], &["note"]);
    let target = ImportTarget {
        driver_id: "mysql",

        capabilities: DriverCapabilities::default(),
        schema: None,
        table: "people",
        columns: &columns,
        mapping: &mapping,
    };

    let plan = build_insert_plan(&target, &sheet, &CsvImportOptions::default()).expect("plan");

    assert_eq!(plan.statement, "INSERT INTO `people` (`note`) VALUES (?)");
    assert_eq!(plan.rows, vec![vec![Value::Text("plain".into())]]);
}

#[test]
fn postgres_enum_import_plan_uses_catalog_type_for_text_rows() {
    let mut status = column("status", "value_contract_enum_schema.value_contract_status");
    status.enum_type = Some(crate::query::QualifiedTypeName {
        schema: "value_contract_enum_schema".into(),
        name: "value_contract_status".into(),
    });
    let columns = vec![status];
    let mapping = vec![Some(0)];
    let sheet = sheet(&[&["NULL"]], &["status"]);

    let plan = build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect("plan");

    assert!(
        plan.statement
            .contains("$1::text::\"value_contract_enum_schema\".\"value_contract_status\"")
    );
    assert_eq!(plan.rows, vec![vec![Value::Text("NULL".into())]]);
}

#[test]
fn postgres_enum_import_requires_a_null_marker_for_empty_fields() {
    let mut status = column("status", "value_contract_status");
    status.enum_type = Some(crate::query::QualifiedTypeName {
        schema: "value_contract_enum_schema".into(),
        name: "value_contract_status".into(),
    });
    let columns = vec![status];
    let mapping = vec![Some(0)];
    let blank_sheet = sheet(&[&[""]], &["status"]);

    let error = build_insert_plan(&target(&columns, &mapping), &blank_sheet, &CsvImportOptions::default())
        .expect_err("a blank could be either NULL or an empty enum label");
    let PlanError::Rows { total, first } = error else {
        panic!("expected the ambiguous blank to be refused");
    };
    assert_eq!(total, 1);
    assert_eq!(first[0].reason, crate::import::CellError::AmbiguousEnumNullOrEmpty);

    let options = CsvImportOptions {
        null_marker: "\\N".into(),
        ..CsvImportOptions::default()
    };
    let explicit_sheet = sheet(&[&[""], &["\\N"], &["NULL"]], &["status"]);
    let plan = build_insert_plan(&target(&columns, &mapping), &explicit_sheet, &options).expect("explicit marker");
    assert_eq!(
        plan.rows,
        vec![
            vec![Value::Text(String::new())],
            vec![Value::Null],
            vec![Value::Text("NULL".into())],
        ]
    );
}

#[test]
fn postgres_enum_array_import_distinguishes_blank_null_from_empty_array() {
    let mut labels = column("labels", "enum_array_schema.label[]");
    labels.enum_type = Some(crate::query::QualifiedTypeName {
        schema: "enum_array_schema".into(),
        name: "label".into(),
    });
    let columns = vec![labels];
    let mapping = vec![Some(0)];
    let sheet = sheet(&[&[""], &["{}"]], &["labels"]);
    let plan = build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default())
        .expect("blank is an unambiguous NULL array; an empty array is `{}`");
    assert_eq!(plan.rows, vec![vec![Value::Null], vec![Value::Text("{}".into())]]);
    assert!(plan.statement.contains("$1::text::\"enum_array_schema\".\"label\"[]"));
}

#[test]
fn value_contract_postgres_temporal_csv_rows_share_text_cast_parameters() {
    let columns = vec![
        column("date_value", "date"),
        column("time_value", "time"),
        column("timetz_value", "timetz"),
        column("timestamp_value", "timestamp"),
        column("timestamptz_value", "timestamptz"),
    ];
    let mapping = vec![Some(0), Some(1), Some(2), Some(3), Some(4)];
    let rows = sheet(
        &[
            &[
                "'-infinity",
                "24:00:00",
                "01:02:03+05:45:12",
                "2001-02-03 04:05:06.000007",
                "2001-02-03T04:05:06.000007+00:00",
            ],
            &[
                "2001-02-03",
                "12:13:14.000015",
                "01:02:03-03:30:12",
                "2001-02-03 04:05:06.000007",
                "2001-02-03T04:05:06.000007+00:00",
            ],
        ],
        &["date", "time", "timetz", "timestamp", "timestamptz"],
    );
    let plan = build_insert_plan(&target(&columns, &mapping), &rows, &CsvImportOptions::default()).unwrap();

    assert_eq!(
        plan.statement,
        "INSERT INTO \"people\" (\"date_value\", \"time_value\", \"timetz_value\", \"timestamp_value\", \"timestamptz_value\") VALUES ($1::text::pg_catalog.date, $2::text::pg_catalog.time, $3::text::pg_catalog.timetz, $4::text::pg_catalog.timestamp, $5::text::pg_catalog.timestamptz)"
    );
    assert_eq!(
        plan.rows,
        vec![
            vec![
                Value::Text("-infinity".into()),
                Value::Text("24:00:00".into()),
                Value::Text("01:02:03+05:45:12".into()),
                Value::Text("2001-02-03 04:05:06.000007".into()),
                Value::Text("2001-02-03T04:05:06.000007+00:00".into()),
            ],
            vec![
                Value::Text("2001-02-03".into()),
                Value::Text("12:13:14.000015".into()),
                Value::Text("01:02:03-03:30:12".into()),
                Value::Text("2001-02-03 04:05:06.000007".into()),
                Value::Text("2001-02-03T04:05:06.000007+00:00".into()),
            ],
        ]
    );

    let mysql_columns = vec![column("date_value", "date")];
    let mysql_mapping = [Some(0)];
    let mysql_rows = sheet(&[&["2001-02-03"]], &["date"]);
    let mysql_target = ImportTarget {
        driver_id: "mysql",

        capabilities: DriverCapabilities::default(),
        schema: None,
        table: "people",
        columns: &mysql_columns,
        mapping: &mysql_mapping,
    };
    let mysql_plan = build_insert_plan(&mysql_target, &mysql_rows, &CsvImportOptions::default()).unwrap();
    assert_eq!(mysql_plan.statement, "INSERT INTO `people` (`date_value`) VALUES (?)");
    assert_eq!(
        mysql_plan.rows,
        vec![vec![Value::Date(chrono::NaiveDate::from_ymd_opt(2001, 2, 3).unwrap())]]
    );
}

#[test]
fn postgres_naive_timestamptz_csv_values_keep_server_session_zone_semantics() {
    let columns = vec![column("happened_at", "timestamptz")];
    let mapping = vec![Some(0)];
    let input = "2024-01-02 03:04:05.123456";
    let rows = sheet(&[&[input]], &["happened_at"]);

    let plan = build_insert_plan(&target(&columns, &mapping), &rows, &CsvImportOptions::default()).expect("plan");

    assert_eq!(
        plan.statement,
        "INSERT INTO \"people\" (\"happened_at\") VALUES ($1::text::pg_catalog.timestamptz)"
    );
    assert_eq!(plan.rows, vec![vec![Value::Text(input.into())]]);
}

#[test]
fn value_contract_postgres_array_csv_insert_uses_allowlisted_native_cast() {
    let columns = vec![column("id", "integer"), column("value", "double precision[]")];
    let mapping = vec![Some(0), Some(1)];
    let rows = sheet(&[&["4", "{1.0000000000000002,-0,5e-324,NULL}"]], &["id", "value"]);
    let plan = build_insert_plan(&target(&columns, &mapping), &rows, &CsvImportOptions::default()).unwrap();

    assert_eq!(
        plan.statement,
        "INSERT INTO \"people\" (\"id\", \"value\") VALUES ($1, $2::text::pg_catalog.float8[])"
    );
    assert_eq!(
        plan.rows,
        vec![vec![
            Value::Int(4),
            Value::Text("{1.0000000000000002,-0,5e-324,NULL}".into())
        ]]
    );
}

#[test]
fn value_contract_postgres_wide_numeric_csv_rows_share_text_cast_parameters() {
    let columns = vec![column("amount", "numeric(65,0)")];
    let mapping = vec![Some(0)];
    let wide = "1234567890123456789012345678901234567890";
    let rows = sheet(&[&["12"], &[wide], &[""]], &["amount"]);
    let plan = build_insert_plan(&target(&columns, &mapping), &rows, &CsvImportOptions::default()).unwrap();

    assert_eq!(
        plan.statement,
        "INSERT INTO \"people\" (\"amount\") VALUES ($1::text::pg_catalog.numeric)"
    );
    assert_eq!(
        plan.rows,
        vec![
            vec![Value::Text("12".into())],
            vec![Value::Text(wide.into())],
            vec![Value::Null],
        ]
    );
}

/// The statement builder drops a column whose value is NULL when the
/// column has a server default. One approved import must send one
/// shape, so the plan builds the statement from values that are never
/// NULL and keeps every mapped column in it.
#[test]
fn a_mapped_column_with_a_server_default_stays_in_the_statement_for_every_row() {
    let mut columns = vec![column("id", "bigint"), column("created", "timestamp")];
    columns[1].default_value = Some("now()".into());
    let mapping = vec![Some(0), Some(1)];
    let sheet = sheet(&[&["1", ""], &["2", "2024-05-06 00:00:00"]], &["id", "created"]);

    let plan = build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect("plan");

    assert!(plan.statement.contains("\"created\""));
    assert_eq!(plan.rows[0][1], Value::Null);
}

#[test]
fn an_unmapped_column_is_left_out_so_its_own_default_applies() {
    let columns = vec![column("id", "bigint"), column("created", "timestamp")];
    let mapping = vec![Some(0), None];
    let sheet = sheet(&[&["1"]], &["id"]);

    let plan = build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect("plan");

    assert!(!plan.statement.contains("created"));
    assert_eq!(plan.columns.len(), 1);
}

#[test]
fn a_mapped_auto_increment_column_keeps_explicit_values() {
    let mut columns = vec![column("id", "bigint"), column("name", "text")];
    columns[0].is_auto_increment = true;
    let mapping = vec![Some(0), Some(1)];
    let sheet = sheet(&[&["901", "ada"]], &["id", "name"]);

    let plan = build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect("plan");

    assert_eq!(
        plan.statement,
        "INSERT INTO \"people\" (\"id\", \"name\") OVERRIDING SYSTEM VALUE VALUES ($1, $2)"
    );
    assert_eq!(
        plan.columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        vec!["id", "name"]
    );
    assert_eq!(plan.rows, vec![vec![Value::Int(901), Value::Text("ada".into())]]);
}

#[test]
fn a_mapped_mysql_auto_increment_column_keeps_explicit_values() {
    let mut columns = vec![column("id", "bigint"), column("name", "text")];
    columns[0].is_auto_increment = true;
    let mapping = vec![Some(0), Some(1)];
    let sheet = sheet(&[&["901", "ada"]], &["id", "name"]);
    let mut target = target(&columns, &mapping);
    target.driver_id = "mysql";

    let plan = build_insert_plan(&target, &sheet, &CsvImportOptions::default()).expect("plan");

    assert_eq!(plan.statement, "INSERT INTO `people` (`id`, `name`) VALUES (?, ?)");
    assert_eq!(plan.rows, vec![vec![Value::Int(901), Value::Text("ada".into())]]);
}

#[test]
fn a_mssql_identity_column_remains_server_owned() {
    let mut columns = vec![column("id", "int"), column("name", "text")];
    columns[0].is_auto_increment = true;
    let mapping = vec![Some(0), Some(1)];
    let sheet = sheet(&[&["901", "ada"]], &["id", "name"]);
    let mut target = target(&columns, &mapping);
    target.driver_id = "mssql";
    target.capabilities = DriverCapabilities::default();

    let plan = build_insert_plan(&target, &sheet, &CsvImportOptions::default()).expect("plan");

    assert_eq!(plan.statement, "INSERT INTO [people] ([name]) VALUES (@P1)");
    assert_eq!(plan.rows, vec![vec![Value::Text("ada".into())]]);
}

#[test]
fn explicit_identity_values_are_refused_when_the_profile_does_not_declare_support() {
    let columns = vec![
        ColumnInfo {
            is_auto_increment: true,
            ..column("id", "bigint")
        },
        column("name", "text"),
    ];
    let mapping = vec![Some(0), Some(1)];
    let sheet = sheet(&[&["901", "ada"]], &["id", "name"]);
    let mut target = target(&columns, &mapping);
    target.capabilities = DriverCapabilities::default();

    let plan = build_insert_plan(&target, &sheet, &CsvImportOptions::default()).expect("plan");

    assert_eq!(plan.statement, "INSERT INTO \"people\" (\"name\") VALUES ($1)");
    assert_eq!(plan.rows, vec![vec![Value::Text("ada".into())]]);
}

#[test]
fn a_generated_column_is_never_written() {
    let mut columns = vec![column("computed", "text"), column("name", "text")];
    columns[0].is_generated = true;
    let mapping = vec![Some(0), Some(1)];
    let sheet = sheet(&[&["must be skipped", "ada"]], &["computed", "name"]);

    let plan = build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect("plan");

    assert_eq!(plan.statement, "INSERT INTO \"people\" (\"name\") VALUES ($1)");
    assert_eq!(
        plan.columns
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        vec!["name"]
    );
    assert_eq!(plan.rows, vec![vec![Value::Text("ada".into())]]);
}

#[test]
fn an_identifier_that_carries_a_quote_is_escaped_rather_than_closed() {
    let columns = vec![column("na\"me", "text")];
    let mapping = vec![Some(0)];
    let mut target = target(&columns, &mapping);
    target.table = "peo\"ple\"; DROP TABLE people; --";
    let rows = sheet(&[&["ada"]], &["name"]);

    let plan = build_insert_plan(&target, &rows, &CsvImportOptions::default()).expect("plan");

    assert_eq!(
        plan.statement,
        "INSERT INTO \"peo\"\"ple\"\"; DROP TABLE people; --\" (\"na\"\"me\") VALUES ($1)"
    );
}

#[test]
fn an_identifier_that_is_empty_or_carries_a_control_character_is_refused() {
    let columns = vec![column("id", "bigint")];
    let mapping = vec![Some(0)];
    let rows = sheet(&[&["1"]], &["id"]);
    let mut empty = target(&columns, &mapping);
    empty.table = "   ";
    assert_eq!(
        build_insert_plan(&empty, &rows, &CsvImportOptions::default()),
        Err(PlanError::Ident(IdentError::Empty))
    );

    let broken = vec![column("na\nme", "text")];
    assert_eq!(
        build_insert_plan(&target(&broken, &mapping), &rows, &CsvImportOptions::default()),
        Err(PlanError::Ident(IdentError::ControlCharacter))
    );
}

#[test]
fn rows_that_cannot_be_read_stop_the_plan_and_are_reported_without_their_contents() {
    let columns = vec![column("id", "bigint")];
    let mapping = vec![Some(0)];
    let sheet = sheet(&[&["1"], &["not-a-number"], &["also-bad"]], &["id"]);

    let error =
        build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect_err("refused");

    let PlanError::Rows { total, first } = error else {
        panic!("expected row failures");
    };
    assert_eq!(total, 2);
    assert_eq!(first[0].line, 3);
    assert!(!first.iter().any(|error| error.to_string().contains("not-a-number")));
}

#[test]
fn numeric_affinity_text_fallback_is_sqlite_only() {
    let columns = vec![column("value", "NUMERIC")];
    let mapping = vec![Some(0)];
    let sheet = sheet(&[&["not-a-number"]], &["value"]);
    let sqlite = ImportTarget {
        driver_id: "sqlite",
        capabilities: DriverCapabilities {
            supports_explicit_auto_increment_values: true,
            ..DriverCapabilities::default()
        },
        schema: None,
        table: "flexible",
        columns: &columns,
        mapping: &mapping,
    };
    let plan = build_insert_plan(&sqlite, &sheet, &CsvImportOptions::default()).expect("SQLite stores text");
    assert_eq!(plan.rows, vec![vec![Value::Text("not-a-number".to_owned())]]);

    let postgres = target(&columns, &mapping);
    assert!(matches!(
        build_insert_plan(&postgres, &sheet, &CsvImportOptions::default()),
        Err(PlanError::Rows { total: 1, .. })
    ));
}

#[test]
fn only_the_first_failures_are_listed_and_the_rest_are_counted() {
    let columns = vec![column("id", "bigint")];
    let mapping = vec![Some(0)];
    let rows: Vec<Vec<String>> = (0..MAX_REPORTED_ROW_ERRORS + 5)
        .map(|_| vec!["bad".to_owned()])
        .collect();
    let sheet = CsvSheet {
        headers: vec!["id".to_owned()],
        rows,
        truncated: false,
    };

    let error =
        build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect_err("refused");

    let PlanError::Rows { total, first } = error else {
        panic!("expected row failures");
    };
    assert_eq!(total, MAX_REPORTED_ROW_ERRORS + 5);
    assert_eq!(first.len(), MAX_REPORTED_ROW_ERRORS);
}

#[test]
fn a_file_with_nothing_mapped_or_nothing_in_it_is_refused() {
    let columns = vec![column("id", "bigint")];
    let filled = sheet(&[&["1"]], &["id"]);
    assert_eq!(
        build_insert_plan(&target(&columns, &[None]), &filled, &CsvImportOptions::default()),
        Err(PlanError::NoMappedColumns)
    );
    assert_eq!(
        build_insert_plan(
            &target(&columns, &[Some(0)]),
            &sheet(&[], &["id"]),
            &CsvImportOptions::default()
        ),
        Err(PlanError::NoRows)
    );
}

#[test]
fn the_batches_a_plan_offers_are_bounded_and_cover_every_row() {
    let columns = vec![column("id", "bigint")];
    let mapping = vec![Some(0)];
    let rows: Vec<Vec<String>> = (0..7).map(|index| vec![index.to_string()]).collect();
    let sheet = CsvSheet {
        headers: vec!["id".to_owned()],
        rows,
        truncated: false,
    };
    let plan = build_insert_plan(&target(&columns, &mapping), &sheet, &CsvImportOptions::default()).expect("plan");

    let batches: Vec<usize> = plan.batches(3).map(<[Vec<Value>]>::len).collect();

    assert_eq!(batches, vec![3, 3, 1]);
    assert_eq!(batches.iter().sum::<usize>(), 7);
}
