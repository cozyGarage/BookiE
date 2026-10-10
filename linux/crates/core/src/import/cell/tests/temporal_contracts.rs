use super::*;

#[test]
fn value_contract_postgres_temporal_sentinels_restore_only_recognized_csv_formula_markers() {
    let cases = [
        ("date", "'-infinity", "-infinity"),
        ("timestamp", "infinity", "infinity"),
        ("timestamptz", "'+infinity", "+infinity"),
        ("time", "24:00:00", "24:00:00"),
        ("timetz", "01:02:03+05:45:12", "01:02:03+05:45:12"),
    ];
    for (data_type, input, expected) in cases {
        let columns = vec![column("value", data_type)];
        let values = row_to_values_for_driver(
            &[input.to_owned()],
            &[Some(0)],
            &columns,
            &CsvImportOptions::default(),
            2,
            "postgres",
        )
        .unwrap_or_else(|error| panic!("PostgreSQL {data_type} sentinel should import: {error}"));
        assert_eq!(values, vec![Value::Text(expected.into())]);
    }

    let columns = vec![column("value", "date")];
    for (input, driver) in [("'=1+1", "postgres"), ("'-infinity", "mysql")] {
        assert!(
            row_to_values_for_driver(
                &[input.into()],
                &[Some(0)],
                &columns,
                &CsvImportOptions::default(),
                2,
                driver,
            )
            .is_err()
        );
    }
}

#[test]
fn value_contract_mysql_invalid_calendar_csv_values_remain_exact_text() {
    let options = CsvImportOptions::default();
    for (data_type, text) in [("DATE", "2024-02-31"), ("DATETIME", "2024-02-31 12:34:56")] {
        let values = row_to_values_for_driver(
            &[text.into()],
            &[Some(0)],
            &[column("value", data_type)],
            &options,
            2,
            "mysql",
        )
        .expect("MySQL invalid calendar values are transported as exact text");
        assert_eq!(values, vec![Value::Text(text.into())]);
    }

    for text in [
        "024-02-31 12:34:56",
        "2024-13-01 12:34:56",
        "2024-02-32 12:34:56",
        "2024-02-31x 12:34:56",
        "2024-02-31 25:00:00",
    ] {
        assert!(
            row_to_values_for_driver(
                &[text.into()],
                &[Some(0)],
                &[column("value", "DATETIME")],
                &options,
                2,
                "mysql",
            )
            .is_err(),
            "malformed calendar text must remain rejected: {text}"
        );
    }
}

#[test]
fn value_contract_postgres_csv_restores_chrono_year_zero_as_bc_era_text() {
    let date = column("value", "date");
    let values = row_to_values_for_driver(
        &["0000-01-02".into()],
        &[Some(0)],
        &[date],
        &CsvImportOptions::default(),
        2,
        "postgres",
    )
    .expect("year zero is PostgreSQL 1 BC");

    assert_eq!(values, vec![Value::Text("0001-01-02 BC".into())]);
}
