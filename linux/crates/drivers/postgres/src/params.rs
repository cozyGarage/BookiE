use sqlx::encode::IsNull;
use sqlx::postgres::{PgArgumentBuffer, PgConnection, PgTypeInfo, PgTypeKind};
use sqlx::{Encode, Executor, Postgres, SqlSafeStr, Statement, Type};
use tablepro_core::{ColumnInfo, DriverError, Value};

use crate::{map_sqlx_error, statement_columns};

pub(super) struct PgParameterDescription {
    pub(super) enum_types: Vec<Option<PgTypeInfo>>,
    pub(super) columns: Vec<ColumnInfo>,
}

pub(super) async fn describe_query_parameters(
    connection: &mut PgConnection,
    sql: &str,
    params: &[Value],
) -> Result<PgParameterDescription, DriverError> {
    let described_sql = format!("{sql}\n/* Bookie parameter type description */");
    let described_sql = sqlx::AssertSqlSafe(described_sql.as_str()).into_sql_str();
    let statement = match connection.prepare(described_sql.clone()).await {
        Ok(statement) => statement,
        Err(error) => {
            let ambiguous = error
                .as_database_error()
                .and_then(|error| error.code())
                .is_some_and(|code| matches!(code.as_ref(), "42P08" | "42P18"));
            if !ambiguous {
                return Err(map_sqlx_error(error));
            }
            let parameter_types = pg_parameter_type_infos(params);
            match connection.prepare_with(described_sql, &parameter_types).await {
                Ok(statement) => statement,
                Err(fallback_error) => {
                    let text_operator_mismatch = fallback_error
                        .as_database_error()
                        .and_then(|error| error.code())
                        .is_some_and(|code| code.as_ref() == "42883");
                    return Err(map_sqlx_error(if text_operator_mismatch {
                        error
                    } else {
                        fallback_error
                    }));
                }
            }
        }
    };
    let parameters = statement
        .parameters()
        .and_then(|types| types.left())
        .unwrap_or_default();
    let enum_types = parameters.iter().map(enum_type_info).collect::<Result<Vec<_>, _>>()?;
    Ok(PgParameterDescription {
        enum_types,
        columns: statement_columns(statement.columns()),
    })
}

fn enum_type_info(type_info: &PgTypeInfo) -> Result<Option<PgTypeInfo>, DriverError> {
    let mut current = type_info.clone();
    let mut domain_depth = 0;
    loop {
        match current.kind() {
            PgTypeKind::Enum(_) => {
                if domain_depth >= 64 {
                    return Err(DriverError::Unsupported(
                        "PostgreSQL enum domain hierarchy exceeds the driver's resolvable depth".into(),
                    ));
                }
                return Ok(Some(current));
            }
            PgTypeKind::Domain(base) => {
                domain_depth += 1;
                current = base.clone();
            }
            _ => return Ok(None),
        }
    }
}

pub(super) fn needs_enum_type_inference(params: &[Value]) -> bool {
    params.iter().any(|param| matches!(param, Value::Null | Value::Text(_)))
}

struct PgEnumParameter<'a> {
    value: Option<&'a str>,
    type_info: PgTypeInfo,
}

impl Type<Postgres> for PgEnumParameter<'_> {
    fn type_info() -> PgTypeInfo {
        <String as Type<Postgres>>::type_info()
    }

    fn compatible(type_info: &PgTypeInfo) -> bool {
        matches!(type_info.kind(), PgTypeKind::Enum(_))
    }
}

impl Encode<'_, Postgres> for PgEnumParameter<'_> {
    fn encode_by_ref(&self, buffer: &mut PgArgumentBuffer) -> Result<IsNull, sqlx::error::BoxDynError> {
        match self.value {
            Some(value) => {
                buffer.extend(value.as_bytes());
                Ok(IsNull::No)
            }
            None => Ok(IsNull::Yes),
        }
    }

    fn produces(&self) -> Option<PgTypeInfo> {
        Some(self.type_info.clone())
    }

    fn size_hint(&self) -> usize {
        self.value.map_or(0, str::len)
    }
}

pub(super) fn bind_pg_params<'q>(
    mut query: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    params: &'q [Value],
    enum_types: &[Option<PgTypeInfo>],
) -> Result<sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>, DriverError> {
    for (index, value) in params.iter().enumerate() {
        let enum_type = enum_types.get(index).and_then(Option::as_ref);
        query = match (value, enum_type) {
            (Value::Null, Some(type_info)) => query.bind(PgEnumParameter {
                value: None,
                type_info: type_info.clone(),
            }),
            (Value::Text(value), Some(type_info)) => query.bind(PgEnumParameter {
                value: Some(value),
                type_info: type_info.clone(),
            }),
            (Value::Null, None) => query.bind(Option::<&str>::None),
            (Value::Bool(value), _) => query.bind(*value),
            (Value::Int(value), _) => query.bind(*value),
            (Value::Float(value), _) => query.bind(*value),
            (Value::Text(value), _) => query.bind(value.clone()),
            (Value::Bytes(value), _) => query.bind(value.clone()),
            (Value::Date(value), _) => query.bind(*value),
            (Value::Time(value), _) => query.bind(*value),
            (Value::DateTime(value), _) => query.bind(*value),
            (Value::TimestampTz(value), _) => query.bind(*value),
            (Value::Decimal(value), _) => query.bind(*value),
            (Value::Uuid(value), _) => query.bind(*value),
            (Value::Json(value), _) => query.bind(value.clone()),
            (Value::Undecodable(_), _) => {
                return Err(DriverError::Unsupported(
                    "undecodable cell cannot be bound as a parameter".into(),
                ));
            }
        };
    }
    Ok(query)
}

pub(super) fn pg_parameter_type_infos(params: &[Value]) -> Vec<PgTypeInfo> {
    params
        .iter()
        .map(|param| match param {
            Value::Null => <Option<&str> as Type<Postgres>>::type_info(),
            Value::Bool(_) => <bool as Type<Postgres>>::type_info(),
            Value::Int(_) => <i64 as Type<Postgres>>::type_info(),
            Value::Float(_) => <f64 as Type<Postgres>>::type_info(),
            Value::Text(_) => <String as Type<Postgres>>::type_info(),
            Value::Bytes(_) => <Vec<u8> as Type<Postgres>>::type_info(),
            Value::Date(_) => <chrono::NaiveDate as Type<Postgres>>::type_info(),
            Value::Time(_) => <chrono::NaiveTime as Type<Postgres>>::type_info(),
            Value::DateTime(_) => <chrono::NaiveDateTime as Type<Postgres>>::type_info(),
            Value::TimestampTz(_) => <chrono::DateTime<chrono::Utc> as Type<Postgres>>::type_info(),
            Value::Decimal(_) => <rust_decimal::Decimal as Type<Postgres>>::type_info(),
            Value::Uuid(_) => <uuid::Uuid as Type<Postgres>>::type_info(),
            Value::Json(_) => <sqlx::types::Json<serde_json::Value> as Type<Postgres>>::type_info(),
            Value::Undecodable(_) => PgTypeInfo::with_name("TEXT"),
        })
        .collect()
}
