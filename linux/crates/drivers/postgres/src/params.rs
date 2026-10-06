use sqlx::encode::IsNull;
use sqlx::postgres::{PgArgumentBuffer, PgConnection, PgTypeInfo, PgTypeKind};
use sqlx::{Encode, Executor, Postgres, SqlSafeStr, Statement, Type};
use tablepro_core::{ColumnInfo, DriverError, Value};

use crate::array::MAX_ARRAY_TEXT_BYTES;
use crate::map_sqlx_error;
use crate::query::{statement_columns, statement_type_infos};

pub(super) struct PgParameterDescription {
    pub(super) inferred_text_types: Vec<Option<PgTypeInfo>>,
    pub(super) columns: Vec<ColumnInfo>,
    pub(super) column_type_infos: Vec<PgTypeInfo>,
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
    let inferred_text_types = parameters
        .iter()
        .map(inferred_text_type_info)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PgParameterDescription {
        inferred_text_types,
        columns: statement_columns(statement.columns()),
        column_type_infos: statement_type_infos(statement.columns()),
    })
}

fn inferred_text_type_info(type_info: &PgTypeInfo) -> Result<Option<PgTypeInfo>, DriverError> {
    inferred_text_type_info_at(type_info, 0)
}

fn inferred_text_type_info_at(type_info: &PgTypeInfo, domain_depth: usize) -> Result<Option<PgTypeInfo>, DriverError> {
    match type_info.kind() {
        PgTypeKind::Enum(_) => {
            if domain_depth >= 64 {
                return Err(DriverError::Unsupported(
                    "PostgreSQL enum domain hierarchy exceeds the driver's resolvable depth".into(),
                ));
            }
            Ok(Some(type_info.clone()))
        }
        PgTypeKind::Domain(base) => inferred_text_type_info_at(base, domain_depth + 1),
        PgTypeKind::Array(element) if is_enum_element(element, 0)? => Ok(Some(type_info.clone())),
        _ => Ok(None),
    }
}

fn is_enum_element(type_info: &PgTypeInfo, domain_depth: usize) -> Result<bool, DriverError> {
    match type_info.kind() {
        PgTypeKind::Enum(_) => {
            if domain_depth >= 64 {
                return Err(DriverError::Unsupported(
                    "PostgreSQL enum domain hierarchy exceeds the driver's resolvable depth".into(),
                ));
            }
            Ok(true)
        }
        PgTypeKind::Domain(base) => is_enum_element(base, domain_depth + 1),
        _ => Ok(false),
    }
}

pub(super) fn needs_enum_type_inference(params: &[Value]) -> bool {
    params.iter().any(|param| matches!(param, Value::Null | Value::Text(_)))
}

struct PgInferredTextParameter<'a> {
    value: Option<&'a str>,
    type_info: PgTypeInfo,
    enum_array: Option<Option<PgArrayText>>,
}

impl<'a> PgInferredTextParameter<'a> {
    fn new(value: Option<&'a str>, type_info: PgTypeInfo) -> Result<Self, DriverError> {
        let enum_array = match type_info.kind() {
            PgTypeKind::Array(element) if is_enum_element(element, 0)? => {
                Some(value.map(parse_enum_array_text).transpose()?)
            }
            _ => None,
        };
        Ok(Self {
            value,
            type_info,
            enum_array,
        })
    }
}

impl Type<Postgres> for PgInferredTextParameter<'_> {
    fn type_info() -> PgTypeInfo {
        <String as Type<Postgres>>::type_info()
    }

    fn compatible(type_info: &PgTypeInfo) -> bool {
        match type_info.kind() {
            PgTypeKind::Enum(_) => true,
            PgTypeKind::Domain(base) => Self::compatible(base),
            PgTypeKind::Array(element) => is_enum_element(element, 0).unwrap_or(false),
            _ => false,
        }
    }
}

impl Encode<'_, Postgres> for PgInferredTextParameter<'_> {
    fn encode_by_ref(&self, buffer: &mut PgArgumentBuffer) -> Result<IsNull, sqlx::error::BoxDynError> {
        if let Some(elements) = &self.enum_array {
            let Some(elements) = elements else {
                return Ok(IsNull::Yes);
            };
            let PgTypeKind::Array(element_type) = self.type_info.kind() else {
                return Err("inferred enum-array parameter lost its array type".into());
            };
            let element_oid = element_type
                .oid()
                .ok_or("inferred enum-array element has no PostgreSQL OID")?;
            let dimensions = i32::try_from(elements.dimensions.len()).map_err(|_| "too many enum-array dimensions")?;
            buffer.extend(&dimensions.to_be_bytes());
            buffer.extend(&0_i32.to_be_bytes());
            buffer.extend(&element_oid.0.to_be_bytes());
            for (length, lower_bound) in &elements.dimensions {
                buffer.extend(&length.to_be_bytes());
                buffer.extend(&lower_bound.to_be_bytes());
            }
            for element in &elements.elements {
                match element {
                    Some(value) => {
                        let length = i32::try_from(value.len()).map_err(|_| "enum label is too large")?;
                        buffer.extend(&length.to_be_bytes());
                        buffer.extend(value.as_bytes());
                    }
                    None => buffer.extend(&(-1_i32).to_be_bytes()),
                }
            }
            return Ok(IsNull::No);
        }
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

struct PgArrayText {
    dimensions: Vec<(i32, i32)>,
    elements: Vec<Option<String>>,
}

enum PgArrayNode {
    Array(Vec<PgArrayNode>),
    Element(Option<String>),
}

fn parse_enum_array_text(text: &str) -> Result<PgArrayText, DriverError> {
    if text.len() > MAX_ARRAY_TEXT_BYTES {
        return Err(DriverError::Unsupported(
            "PostgreSQL enum-array parameter exceeds the 16 MiB limit".into(),
        ));
    }
    let mut parser = PgArrayTextParser {
        chars: text.chars().peekable(),
    };
    let declared_dimensions = parser.dimensions()?;
    let node = parser.array()?;
    if parser.chars.next().is_some() {
        return Err(DriverError::Unsupported(
            "invalid PostgreSQL enum-array parameter syntax".into(),
        ));
    }
    let (shape, elements) = flatten_enum_array(node)?;
    if shape.len() > 6 {
        return Err(DriverError::Unsupported(
            "PostgreSQL enum arrays support at most six dimensions".into(),
        ));
    }
    let lengths = shape
        .iter()
        .map(|length| {
            i32::try_from(*length).map_err(|_| DriverError::Unsupported("enum array dimension is too large".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let dimensions = if lengths == [0] && declared_dimensions.is_empty() {
        Vec::new()
    } else if declared_dimensions.is_empty() {
        lengths.into_iter().map(|length| (length, 1)).collect()
    } else {
        if declared_dimensions
            .iter()
            .map(|(length, _)| *length)
            .collect::<Vec<_>>()
            != lengths
        {
            return Err(DriverError::Unsupported(
                "enum-array bounds do not match the array contents".into(),
            ));
        }
        declared_dimensions
    };
    Ok(PgArrayText { dimensions, elements })
}

struct PgArrayTextParser<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl PgArrayTextParser<'_> {
    fn dimensions(&mut self) -> Result<Vec<(i32, i32)>, DriverError> {
        let mut dimensions = Vec::new();
        while self.chars.peek() == Some(&'[') {
            self.chars.next();
            let lower = self.integer()?;
            if self.chars.next() != Some(':') {
                return Err(DriverError::Unsupported("invalid PostgreSQL enum-array bounds".into()));
            }
            let upper = self.integer()?;
            if self.chars.next() != Some(']') {
                return Err(DriverError::Unsupported("invalid PostgreSQL enum-array bounds".into()));
            }
            let length = upper
                .checked_sub(lower)
                .and_then(|length| length.checked_add(1))
                .filter(|length| *length >= 0)
                .ok_or_else(|| DriverError::Unsupported("invalid PostgreSQL enum-array bounds".into()))?;
            dimensions.push((length, lower));
            if dimensions.len() > 6 {
                return Err(DriverError::Unsupported(
                    "PostgreSQL enum arrays support at most six dimensions".into(),
                ));
            }
        }
        if !dimensions.is_empty() && self.chars.next() != Some('=') {
            return Err(DriverError::Unsupported("invalid PostgreSQL enum-array bounds".into()));
        }
        Ok(dimensions)
    }

    fn integer(&mut self) -> Result<i32, DriverError> {
        let mut text = String::new();
        while self
            .chars
            .peek()
            .is_some_and(|character| character.is_ascii_digit() || *character == '-')
        {
            text.push(self.chars.next().unwrap_or_default());
        }
        text.parse()
            .map_err(|_| DriverError::Unsupported("invalid PostgreSQL enum-array bounds".into()))
    }

    fn array(&mut self) -> Result<PgArrayNode, DriverError> {
        self.array_at_depth(1)
    }

    fn array_at_depth(&mut self, depth: usize) -> Result<PgArrayNode, DriverError> {
        if depth > 6 {
            return Err(DriverError::Unsupported(
                "PostgreSQL enum arrays support at most six dimensions".into(),
            ));
        }
        if self.chars.next() != Some('{') {
            return Err(DriverError::Unsupported(
                "PostgreSQL enum-array parameters require brace syntax".into(),
            ));
        }
        let mut values = Vec::new();
        if self.chars.peek() == Some(&'}') {
            self.chars.next();
            return Ok(PgArrayNode::Array(values));
        }
        loop {
            values.push(if self.chars.peek() == Some(&'{') {
                self.array_at_depth(depth + 1)?
            } else {
                PgArrayNode::Element(self.element()?)
            });
            match self.chars.next() {
                Some(',') => {}
                Some('}') => return Ok(PgArrayNode::Array(values)),
                _ => {
                    return Err(DriverError::Unsupported(
                        "invalid PostgreSQL enum-array parameter syntax".into(),
                    ));
                }
            }
        }
    }

    fn element(&mut self) -> Result<Option<String>, DriverError> {
        let quoted = self.chars.peek() == Some(&'"');
        let mut value = String::new();
        if quoted {
            self.chars.next();
            loop {
                match self.chars.next() {
                    Some('"') => break,
                    Some('\\') => value.push(
                        self.chars
                            .next()
                            .ok_or_else(|| DriverError::Unsupported("invalid PostgreSQL enum-array escape".into()))?,
                    ),
                    Some(character) => value.push(character),
                    None => {
                        return Err(DriverError::Unsupported(
                            "unterminated quoted PostgreSQL enum-array label".into(),
                        ));
                    }
                }
            }
        } else {
            while let Some(&character) = self.chars.peek() {
                if matches!(character, ',' | '}') {
                    break;
                }
                self.chars.next();
                if character == '\\' {
                    value.push(
                        self.chars
                            .next()
                            .ok_or_else(|| DriverError::Unsupported("invalid PostgreSQL enum-array escape".into()))?,
                    );
                } else if matches!(character, '{' | '"' | '[' | ']') {
                    return Err(DriverError::Unsupported("invalid PostgreSQL enum-array element".into()));
                } else {
                    value.push(character);
                }
            }
            value = value.trim().to_owned();
        }
        Ok((quoted || !value.eq_ignore_ascii_case("NULL")).then_some(value))
    }
}

fn flatten_enum_array(node: PgArrayNode) -> Result<(Vec<usize>, Vec<Option<String>>), DriverError> {
    let PgArrayNode::Array(values) = node else {
        return Err(DriverError::Unsupported(
            "PostgreSQL enum-array root must be an array".into(),
        ));
    };
    let Some(first) = values.first() else {
        return Ok((vec![0], Vec::new()));
    };
    if matches!(first, PgArrayNode::Element(_)) {
        if values.iter().any(|value| !matches!(value, PgArrayNode::Element(_))) {
            return Err(DriverError::Unsupported(
                "ragged PostgreSQL enum arrays are unsupported".into(),
            ));
        }
        let length = values.len();
        let elements = values
            .into_iter()
            .map(|value| match value {
                PgArrayNode::Element(value) => value,
                PgArrayNode::Array(_) => None,
            })
            .collect();
        return Ok((vec![length], elements));
    }
    let outer_length = values.len();
    let mut child_shape = None;
    let mut elements = Vec::new();
    for value in values {
        let (shape, mut child_elements) = flatten_enum_array(value)?;
        if child_shape.as_ref().is_some_and(|expected| expected != &shape) {
            return Err(DriverError::Unsupported(
                "ragged PostgreSQL enum arrays are unsupported".into(),
            ));
        }
        child_shape = Some(shape);
        elements.append(&mut child_elements);
    }
    let mut shape = child_shape.unwrap_or_default();
    shape.insert(0, outer_length);
    Ok((shape, elements))
}

pub(super) fn bind_pg_params<'q>(
    mut query: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    params: &'q [Value],
    inferred_text_types: &[Option<PgTypeInfo>],
) -> Result<sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>, DriverError> {
    for (index, value) in params.iter().enumerate() {
        let inferred_type = inferred_text_types.get(index).and_then(Option::as_ref);
        query = match (value, inferred_type) {
            (Value::Null, Some(type_info)) => query.bind(PgInferredTextParameter::new(None, type_info.clone())?),
            (Value::Text(value), Some(type_info)) => {
                query.bind(PgInferredTextParameter::new(Some(value), type_info.clone())?)
            }
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

#[cfg(test)]
mod tests {
    use super::parse_enum_array_text;
    use tablepro_core::DriverError;

    #[test]
    fn inferred_enum_array_text_rejects_nesting_beyond_postgres_dimension_limit() {
        assert!(parse_enum_array_text("{{{{{{ready}}}}}}").is_ok());

        let deeply_nested = format!("{}ready{}", "{".repeat(10_000), "}".repeat(10_000));

        assert!(matches!(
            parse_enum_array_text(&deeply_nested),
            Err(DriverError::Unsupported(message)) if message.contains("at most six dimensions")
        ));
    }

    #[test]
    fn inferred_enum_array_text_preserves_quotes_escapes_nulls_and_bounds() {
        let parsed = parse_enum_array_text(r#"[0:5]={"NULL",NULL,"","a,b","a\"b","a\\b"}"#).unwrap();
        assert_eq!(parsed.dimensions, vec![(6, 0)]);
        assert_eq!(
            parsed.elements,
            vec![
                Some("NULL".into()),
                None,
                Some(String::new()),
                Some("a,b".into()),
                Some("a\"b".into()),
                Some("a\\b".into()),
            ]
        );

        let parsed = parse_enum_array_text(r#"[-1:0][3:4]={{ready,NULL},{"",東京}}"#).unwrap();
        assert_eq!(parsed.dimensions, vec![(2, -1), (2, 3)]);
        assert_eq!(
            parsed.elements,
            vec![Some("ready".into()), None, Some(String::new()), Some("東京".into()),]
        );
    }

    #[test]
    fn inferred_enum_array_text_refuses_malformed_shapes_and_trailing_data() {
        for text in [
            "{{ready},{paused,ready}}",
            "[0:2]={ready,paused}",
            "{} trailing",
            r#"{"unterminated\}"#,
            "{ready,paused]",
        ] {
            assert!(
                matches!(parse_enum_array_text(text), Err(DriverError::Unsupported(_))),
                "malformed array text should be refused: {text:?}"
            );
        }
    }
}
