use super::{set_cells_message, value_input_message};
use crate::set_cells::SetCellsError;
use crate::value_input::ValueInputError;

fn every_value_input_error() -> Vec<ValueInputError> {
    vec![
        ValueInputError::FieldRequired,
        ValueInputError::InvalidEnumMetadata,
        ValueInputError::UndeclaredEnumMember,
        ValueInputError::InvalidBoolean,
        ValueInputError::InvalidInteger,
        ValueInputError::InvalidNumber,
        ValueInputError::InvalidDecimal,
        ValueInputError::InvalidUuid,
        ValueInputError::InvalidJson("expected value at line 1".into()),
        ValueInputError::InvalidTimestamp,
        ValueInputError::InvalidTimestampWithTimeZone,
        ValueInputError::InvalidDateTime,
        ValueInputError::InvalidDateAndTime,
        ValueInputError::InvalidDate,
        ValueInputError::InvalidTime,
        ValueInputError::InvalidSqlServerDateTimeOffsetColumnScale,
        ValueInputError::InvalidSqlServerDateTimeOffset,
        ValueInputError::SqlServerDateTimeOffsetOutOfRange,
        ValueInputError::MySqlSpatialValueNotEditable,
        ValueInputError::MySqlIntegerOutOfRange,
        ValueInputError::MongoInt32OutOfRange,
        ValueInputError::MongoDatePrecision,
        ValueInputError::DuckDbTimestampTzPrecision,
        ValueInputError::DuckDbTemporalPrecision { kind: "TIME" },
        ValueInputError::BitWidthOutOfRange,
    ]
}

#[test]
fn every_value_input_error_has_its_own_readable_message() {
    let mut messages: Vec<String> = every_value_input_error().iter().map(value_input_message).collect();
    assert!(messages.iter().all(|message| !message.trim().is_empty()));
    messages.sort();
    messages.dedup();
    assert_eq!(
        messages.len(),
        every_value_input_error().len(),
        "messages must be distinct"
    );
}

#[test]
fn a_message_carries_the_detail_the_error_holds() {
    assert!(value_input_message(&ValueInputError::InvalidJson("bad token".into())).contains("bad token"));
    assert!(value_input_message(&ValueInputError::DuckDbTemporalPrecision { kind: "TIME" }).contains("TIME"));
    assert!(!value_input_message(&ValueInputError::InvalidJson("x".into())).contains("{error}"));
}

#[test]
fn a_set_cells_message_names_the_column_and_counts_rows_from_one() {
    let read_only = set_cells_message(&SetCellsError::ReadOnlyColumn { column: "id".into() });
    assert!(
        read_only.contains("id") && read_only.contains("No cells were changed"),
        "{read_only}"
    );
    let null = set_cells_message(&SetCellsError::NotNullable { column: "name".into() });
    assert!(null.contains("name") && null.contains("NULL"), "{null}");
    let position = set_cells_message(&SetCellsError::NotEditable {
        column: "blob".into(),
        position: 0,
    });
    assert!(position.contains("blob") && position.contains("row 1"), "{position}");
    let invalid = set_cells_message(&SetCellsError::InvalidValue {
        column: "age".into(),
        error: ValueInputError::InvalidInteger,
    });
    assert!(
        invalid.contains("age") && invalid.contains("Invalid integer"),
        "{invalid}"
    );
    for plain in [
        SetCellsError::TableNotEditable,
        SetCellsError::NothingSelected,
        SetCellsError::TooManyCells,
    ] {
        let message = set_cells_message(&plain);
        assert!(!message.contains('{'), "{message}");
    }
}
