use thiserror::Error;

#[derive(Debug, Error)]
pub enum BuildSqlError {
    #[error("column {column} requires a ClickHouse Date between 1970-01-01 and 2149-06-06")]
    ClickhouseDateOutOfRange { column: String },
    #[error("table has no primary key")]
    NoPrimaryKey,

    #[error("nothing to update")]
    NothingToUpdate,

    #[error("new_values length {got} does not match columns length {expected}")]
    LengthMismatch { expected: usize, got: usize },

    #[error("column {column} contains a value that cannot be represented in a SQL statement")]
    UnrepresentableValue { column: String },

    #[error("the table's columns changed since this edit was made; reload and reapply your changes")]
    StaleColumns,

    #[error("optimistic updates are unsupported for driver {0}")]
    UnsupportedDriver(String),

    #[error("driver {0} cannot set a column to its DEFAULT")]
    DefaultUnsupported(String),

    #[error("an expression must not be empty")]
    EmptyExpression,
}
