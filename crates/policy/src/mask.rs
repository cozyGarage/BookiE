use tablepro_core::{QueryResult, Value};

pub const DEFAULT_SENSITIVE_PATTERNS: &[&str] = &[
    "*pan*",
    "*card*",
    "*cvv*",
    "*cvc*",
    "*ssn*",
    "*iban*",
    "*secret*",
    "*token*",
    "*password*",
    "*passwd*",
    "*api_key*",
    "*apikey*",
    "*private_key*",
];

const REDACTED: &str = "***REDACTED***";

#[derive(Debug, Clone, Default)]
pub(crate) struct SensitivePatterns(Vec<glob::Pattern>);

impl SensitivePatterns {
    pub(crate) fn new(patterns: &[String]) -> Self {
        Self(
            patterns
                .iter()
                .filter_map(|pattern| glob::Pattern::new(&pattern.to_lowercase()).ok())
                .collect(),
        )
    }

    pub(crate) fn matches(&self, column_name: &str) -> bool {
        let name = column_name.to_lowercase();
        self.0.iter().any(|pattern| pattern.matches(&name))
    }

    fn redact_nested(&self, cell: &mut Value) {
        match cell {
            Value::Json(value) => {
                self.redact_json(value);
            }
            Value::JsonText(text) => {
                let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) else {
                    return;
                };
                if self.redact_json(&mut value)
                    && let Ok(redacted) = serde_json::to_string(&value)
                {
                    *text = redacted;
                }
            }
            _ => {}
        }
    }

    fn redact_json(&self, value: &mut serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(map) => {
                let mut changed = false;
                for (key, entry) in map.iter_mut() {
                    if self.matches(key) && !entry.is_null() {
                        *entry = serde_json::Value::String(REDACTED.into());
                        changed = true;
                    } else {
                        changed |= self.redact_json(entry);
                    }
                }
                changed
            }
            serde_json::Value::Array(items) => items
                .iter_mut()
                .fold(false, |changed, item| self.redact_json(item) | changed),
            _ => false,
        }
    }

    pub(crate) fn mask(&self, mut result: QueryResult, sensitive_positions: Option<&[bool]>) -> QueryResult {
        let sensitive: Vec<bool> = result
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| self.matches(&c.name) || sensitive_positions.and_then(|s| s.get(i)).copied().unwrap_or(false))
            .collect();
        if self.0.is_empty() && !sensitive.iter().any(|s| *s) {
            return result;
        }
        for row in &mut result.rows {
            for (i, cell) in row.iter_mut().enumerate() {
                if matches!(cell, Value::Null) {
                    continue;
                }
                if sensitive.get(i).copied().unwrap_or(false) {
                    *cell = Value::Text(REDACTED.into());
                } else {
                    self.redact_nested(cell);
                }
            }
        }
        result
    }
}

pub fn column_is_sensitive(column_name: &str, patterns: &[String]) -> bool {
    SensitivePatterns::new(patterns).matches(column_name)
}

/// Mask sensitive columns in a result set. Non-null values become the
/// redaction sentinel; Null stays Null.
///
/// `sensitive_positions`, when present, marks output columns known to derive
/// from a sensitive source column via the parsed statement's projection (see
/// `classify::sensitive_projection`) — it is ORed with the name-based match
/// so an alias or expression that hides the source column name still gets
/// masked. `patterns` is the caller's fully effective pattern list; this
/// function performs no defaulting of its own.
pub fn apply_masking(result: QueryResult, patterns: &[String], sensitive_positions: Option<&[bool]>) -> QueryResult {
    SensitivePatterns::new(patterns).mask(result, sensitive_positions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tablepro_core::ColumnInfo;

    #[test]
    fn matches_pan_column() {
        let patterns: Vec<String> = DEFAULT_SENSITIVE_PATTERNS.iter().map(|s| (*s).into()).collect();
        assert!(column_is_sensitive("card_pan", &patterns));
        assert!(column_is_sensitive("CVV", &patterns));
        assert!(!column_is_sensitive("amount", &patterns));
    }

    #[test]
    fn redacts_cells() {
        let patterns = vec!["*pan*".into()];
        let result = QueryResult {
            columns: vec![
                ColumnInfo {
                    name: "id".into(),
                    data_type: "int".into(),
                    nullable: false,
                    primary_key: true,
                    is_auto_increment: false,
                    default_value: None,
                    is_generated: false,
                    comment: None,
                    collation: None,
                    enum_type: None,
                    domain_type: None,
                },
                ColumnInfo {
                    name: "pan".into(),
                    data_type: "text".into(),
                    nullable: true,
                    primary_key: false,
                    is_auto_increment: false,
                    default_value: None,
                    is_generated: false,
                    comment: None,
                    collation: None,
                    enum_type: None,
                    domain_type: None,
                },
            ],
            rows: vec![vec![Value::Int(1), Value::Text("4111111111111111".into())]],
            truncated: false,
        };
        let masked = apply_masking(result, &patterns, None);
        assert_eq!(masked.rows[0][0], Value::Int(1));
        assert_eq!(masked.rows[0][1], Value::Text(REDACTED.into()));
    }

    #[test]
    fn an_empty_pattern_list_with_no_known_positions_redacts_nothing() {
        let result = QueryResult {
            columns: vec![ColumnInfo {
                name: "password".into(),
                data_type: "text".into(),
                nullable: true,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
                comment: None,
                collation: None,
                enum_type: None,
                domain_type: None,
            }],
            rows: vec![vec![Value::Text("secret".into())]],
            truncated: false,
        };
        let masked = apply_masking(result, &[], None);
        assert_eq!(masked.rows[0][0], Value::Text("secret".into()));
    }

    #[test]
    fn a_known_sensitive_position_is_redacted_even_when_its_name_does_not_match() {
        let result = QueryResult {
            columns: vec![ColumnInfo {
                name: "p".into(),
                data_type: "text".into(),
                nullable: true,
                primary_key: false,
                is_auto_increment: false,
                default_value: None,
                is_generated: false,
                comment: None,
                collation: None,
                enum_type: None,
                domain_type: None,
            }],
            rows: vec![vec![Value::Text("4111111111111111".into())]],
            truncated: false,
        };
        let masked = apply_masking(result, &[], Some(&[true]));
        assert_eq!(masked.rows[0][0], Value::Text(REDACTED.into()));
    }

    #[test]
    fn redacts_sensitive_keys_nested_in_json_cells() {
        let patterns = vec!["*password*".into(), "*ssn*".into()];
        let column = |name: &str| ColumnInfo {
            name: name.into(),
            data_type: "jsonb".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        };
        let result = QueryResult {
            columns: vec![column("auth"), column("profile")],
            rows: vec![vec![
                Value::Json(Box::new(
                    serde_json::json!({"user": "ada", "creds": [{"Password": "p"}]}),
                )),
                Value::JsonText(r#"{"name":"ada","ssn":"123-45-6789"}"#.into()),
            ]],
            truncated: false,
        };

        let masked = apply_masking(result, &patterns, None);

        let Value::Json(auth) = &masked.rows[0][0] else {
            panic!("a JSON cell stays JSON");
        };
        assert_eq!(auth["user"], "ada");
        assert_eq!(auth["creds"][0]["Password"], REDACTED);
        let Value::JsonText(profile) = &masked.rows[0][1] else {
            panic!("a JSON text cell stays JSON text");
        };
        let profile: serde_json::Value = serde_json::from_str(profile).unwrap();
        assert_eq!(profile["name"], "ada");
        assert_eq!(profile["ssn"], REDACTED);
    }
}
