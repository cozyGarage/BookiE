use crate::query::Value;
use crate::sql_dialect::placeholder_for;
use crate::sql_lex::skip_span_at;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedParameters {
    pub sql: String,
    pub names: Vec<String>,
    pub bindings: Vec<String>,
}

impl NamedParameters {
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParameterKind {
    #[default]
    Auto,
    Text,
    Integer,
    Decimal,
    Boolean,
    Null,
}

impl ParameterKind {
    pub const ALL: [Self; 6] = [
        Self::Auto,
        Self::Text,
        Self::Integer,
        Self::Decimal,
        Self::Boolean,
        Self::Null,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Text => "Text",
            Self::Integer => "Integer",
            Self::Decimal => "Decimal",
            Self::Boolean => "Boolean",
            Self::Null => "Null",
        }
    }

    pub fn from_index(index: u32) -> Self {
        Self::ALL.get(index as usize).copied().unwrap_or(Self::Auto)
    }

    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0) as u32
    }
}

pub fn parse_parameter_value(kind: ParameterKind, text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    match kind {
        ParameterKind::Null => Ok(Value::Null),
        ParameterKind::Text => Ok(Value::Text(text.to_string())),
        ParameterKind::Integer => trimmed
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| format!("{trimmed} is not a whole number")),
        ParameterKind::Decimal => parse_decimal_parameter(trimmed)
            .map(Value::Decimal)
            .map_err(|_| format!("{trimmed} is not an exactly representable decimal number")),
        ParameterKind::Boolean => match trimmed.to_ascii_lowercase().as_str() {
            "true" | "t" | "1" | "yes" => Ok(Value::Bool(true)),
            "false" | "f" | "0" | "no" => Ok(Value::Bool(false)),
            _ => Err(format!("{trimmed} is not true or false")),
        },
        ParameterKind::Auto => Ok(infer_parameter_value(text)),
    }
}

fn parse_decimal_parameter(text: &str) -> Result<rust_decimal::Decimal, rust_decimal::Error> {
    if let Some((mantissa, _)) = text.split_once(['e', 'E']) {
        rust_decimal::Decimal::from_str_exact(mantissa)?;
        rust_decimal::Decimal::from_scientific(text)
    } else {
        rust_decimal::Decimal::from_str_exact(text)
    }
}

fn infer_parameter_value(text: &str) -> Value {
    let trimmed = text.trim();
    if let Ok(value) = trimmed.parse::<i64>() {
        return Value::Int(value);
    }
    if (trimmed.contains('.') || trimmed.contains('e') || trimmed.contains('E'))
        && let Ok(value) = parse_decimal_parameter(trimmed)
    {
        return Value::Decimal(value);
    }
    Value::Text(text.to_string())
}

pub fn extract_named_parameters(sql: &str, driver_id: &str) -> NamedParameters {
    let mut out = String::with_capacity(sql.len());
    let mut names: Vec<String> = Vec::new();
    let mut bindings: Vec<String> = Vec::new();
    let bytes = sql.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        let rest = &sql[index..];

        if let Some(length) = skip_span_at(sql, index, driver_id).filter(|length| *length > 0) {
            out.push_str(&sql[index..index + length]);
            index += length;
            continue;
        }

        if bytes[index] == b':' {
            if rest.starts_with("::") || rest.starts_with(":=") {
                out.push_str(&sql[index..index + 2]);
                index += 2;
                continue;
            }
            if let Some(name) = parameter_name(&rest[1..]) {
                let position = bindings.len();
                bindings.push(name.clone());
                if !names.contains(&name) {
                    names.push(name.clone());
                }
                out.push_str(&placeholder_for(driver_id, position));
                index += 1 + name.len();
                continue;
            }
        }

        let character = rest.chars().next().unwrap_or(':');
        out.push(character);
        index += character.len_utf8();
    }

    NamedParameters {
        sql: out,
        names,
        bindings,
    }
}

fn parameter_name(rest: &str) -> Option<String> {
    let mut end = 0usize;
    for (offset, character) in rest.char_indices() {
        let valid = if offset == 0 {
            character.is_ascii_alphabetic() || character == '_'
        } else {
            character.is_ascii_alphanumeric() || character == '_'
        };
        if !valid {
            break;
        }
        end = offset + character.len_utf8();
    }
    (end > 0).then(|| rest[..end].to_string())
}

#[cfg(test)]
#[path = "params_tests.rs"]
mod tests;

#[cfg(test)]
mod boundary_tests {

    #[test]
    fn a_parameter_after_a_dollar_identifier_is_still_found() {
        let parsed = extract_named_parameters("SELECT my$a$b FROM t WHERE id = :id", "postgres");
        assert_eq!(parsed.names, ["id"]);
        assert_eq!(parsed.sql, "SELECT my$a$b FROM t WHERE id = $1");
    }

    use super::tests::extract_named_parameters;
    use super::*;

    #[test]
    fn value_contract_boolean_parameters_preserve_both_truth_values_and_reject_other_text() {
        for text in ["true", "t", "1", "yes", " TRUE ", "YeS"] {
            assert_eq!(
                parse_parameter_value(ParameterKind::Boolean, text).unwrap(),
                Value::Bool(true)
            );
        }
        for text in ["false", "f", "0", "no", " FALSE ", "nO"] {
            assert_eq!(
                parse_parameter_value(ParameterKind::Boolean, text).unwrap(),
                Value::Bool(false)
            );
        }
        for text in ["", "null", "2", "-1", "0.0", "truthy"] {
            assert!(parse_parameter_value(ParameterKind::Boolean, text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn value_contract_explicit_text_keeps_whitespace_and_placeholder_spelling() {
        for text in [
            "",
            " ",
            "\r\n\t",
            "NULL",
            "00012",
            "-0",
            "1e3",
            ":id ? $1 @P1",
            "x'); DROP TABLE t; --",
        ] {
            assert_eq!(
                parse_parameter_value(ParameterKind::Text, text).unwrap(),
                Value::Text(text.into())
            );
        }
    }

    #[test]
    fn value_contract_parameter_lexer_preserves_nested_and_escaped_regions() {
        for (driver, source, expected) in [
            (
                "postgres",
                "SELECT /* outer :x /* inner :y */ :z */ :a, $tag$:hidden$tag$, :a::text",
                "SELECT /* outer :x /* inner :y */ :z */ $1, $tag$:hidden$tag$, $2::text",
            ),
            (
                "mysql",
                "SELECT 'x\\' :hidden', :a, `:column`, :a # :ignored\n",
                "SELECT 'x\\' :hidden', ?, `:column`, ? # :ignored\n",
            ),
            (
                "mssql",
                "SELECT [a]]:hidden], N'it''s :hidden', :a, :a",
                "SELECT [a]]:hidden], N'it''s :hidden', @P1, @P2",
            ),
        ] {
            let parsed = extract_named_parameters(source, driver);
            assert_eq!(parsed.sql, expected, "{driver}");
            assert_eq!(parsed.names, ["a"], "{driver}");
            assert_eq!(parsed.bindings, ["a", "a"], "{driver}");
        }
    }
}
