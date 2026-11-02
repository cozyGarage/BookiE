use duckdb::core::{LogicalTypeHandle, LogicalTypeId};

pub(crate) fn of(logical_type: &LogicalTypeHandle) -> String {
    match logical_type.id() {
        LogicalTypeId::Boolean => "BOOLEAN".into(),
        LogicalTypeId::Tinyint => "TINYINT".into(),
        LogicalTypeId::Smallint => "SMALLINT".into(),
        LogicalTypeId::Integer => "INTEGER".into(),
        LogicalTypeId::Bigint => "BIGINT".into(),
        LogicalTypeId::Hugeint => "HUGEINT".into(),
        LogicalTypeId::UTinyint => "UTINYINT".into(),
        LogicalTypeId::USmallint => "USMALLINT".into(),
        LogicalTypeId::UInteger => "UINTEGER".into(),
        LogicalTypeId::UBigint => "UBIGINT".into(),
        LogicalTypeId::UHugeint => "UHUGEINT".into(),
        LogicalTypeId::Float => "FLOAT".into(),
        LogicalTypeId::Double => "DOUBLE".into(),
        LogicalTypeId::Decimal => format!(
            "DECIMAL({},{})",
            logical_type.decimal_width(),
            logical_type.decimal_scale()
        ),
        LogicalTypeId::Varchar => "VARCHAR".into(),
        LogicalTypeId::Blob => "BLOB".into(),
        LogicalTypeId::Bit => "BIT".into(),
        LogicalTypeId::Uuid => "UUID".into(),
        LogicalTypeId::Date => "DATE".into(),
        LogicalTypeId::Time => "TIME".into(),
        LogicalTypeId::TimeTZ => "TIME WITH TIME ZONE".into(),
        LogicalTypeId::Timestamp => "TIMESTAMP".into(),
        LogicalTypeId::TimestampS => "TIMESTAMP_S".into(),
        LogicalTypeId::TimestampMs => "TIMESTAMP_MS".into(),
        LogicalTypeId::TimestampNs => "TIMESTAMP_NS".into(),
        LogicalTypeId::TimestampTZ => "TIMESTAMP WITH TIME ZONE".into(),
        LogicalTypeId::Interval => "INTERVAL".into(),
        LogicalTypeId::Enum => "ENUM".into(),
        LogicalTypeId::List | LogicalTypeId::Array => format!("{}[]", of(&logical_type.child(0))),
        LogicalTypeId::Map => format!("MAP({}, {})", of(&logical_type.child(0)), of(&logical_type.child(1))),
        LogicalTypeId::Struct => format!("STRUCT({})", members(logical_type)),
        LogicalTypeId::Union => format!("UNION({})", members(logical_type)),
        other => format!("{other:?}").to_uppercase(),
    }
}

fn members(logical_type: &LogicalTypeHandle) -> String {
    (0..logical_type.num_children())
        .map(|index| format!("{} {}", logical_type.child_name(index), of(&logical_type.child(index))))
        .collect::<Vec<_>>()
        .join(", ")
}
