use std::collections::HashMap;
use std::sync::Arc;

use bookie_core::FilterValue;
use bookie_results::highlight_match::{
    HighlightError, HighlightOp, HighlightRule, HighlightRuleSet, HighlightTarget, MAX_LIKE_PATTERN_BYTES,
    PreparedHighlightRule, highlight_color, prepare_rules,
};
use uuid::Uuid;

use super::config_io::xdg_config_path;
use super::state_file::StateFile;

const FILE: &str = "highlight_settings.json";

type Tables = HashMap<String, HighlightRuleSet>;
type Schemas = HashMap<String, Tables>;
type Connections = HashMap<String, Schemas>;

#[derive(Clone)]
pub struct HighlightSettingsStore {
    file: Arc<StateFile<Connections>>,
}

fn schema_key(schema: Option<&str>) -> String {
    schema.unwrap_or("").to_string()
}

pub fn prepared_rules_for_columns(set: &HighlightRuleSet, columns: &[&str]) -> Arc<[PreparedHighlightRule]> {
    let mut prepared = Vec::with_capacity(set.rules.len());
    for rule in &set.rules {
        if let Ok(mut one) = prepare_rules(std::slice::from_ref(rule), columns) {
            prepared.append(&mut one);
        }
    }
    prepared.into()
}

pub fn default_highlight_rule(column: &str) -> HighlightRule {
    HighlightRule {
        column: column.to_string(),
        op: HighlightOp::Eq,
        value: Some(FilterValue::Single(String::new())),
        color: "red".into(),
        target: HighlightTarget::Cell,
    }
}

pub fn normalize_highlight_rules(rules: &[HighlightRule]) -> HighlightRuleSet {
    HighlightRuleSet {
        rules: rules.iter().cloned().filter_map(normalize_highlight_rule).collect(),
    }
}

pub fn validate_highlight_pattern_lengths(
    original: &HighlightRuleSet,
    candidate: &HighlightRuleSet,
) -> Result<(), HighlightError> {
    let mut preserved = original
        .rules
        .iter()
        .filter_map(like_pattern)
        .filter(|(op, pattern)| is_like(*op) && pattern.len() > MAX_LIKE_PATTERN_BYTES)
        .collect::<Vec<_>>();
    for rule in &candidate.rules {
        let Some((op, pattern)) =
            like_pattern(rule).filter(|(op, pattern)| is_like(*op) && pattern.len() > MAX_LIKE_PATTERN_BYTES)
        else {
            continue;
        };
        let Some(index) = preserved
            .iter()
            .position(|(old_op, old_pattern)| *old_op == op && *old_pattern == pattern)
        else {
            return Err(HighlightError::PatternTooLong);
        };
        preserved.swap_remove(index);
    }
    Ok(())
}

fn is_like(op: HighlightOp) -> bool {
    matches!(op, HighlightOp::Like | HighlightOp::NotLike | HighlightOp::Ilike)
}

fn like_pattern(rule: &HighlightRule) -> Option<(HighlightOp, &str)> {
    match rule.value.as_ref()? {
        FilterValue::Single(pattern) => Some((rule.op, pattern)),
        _ => None,
    }
}

fn normalize_highlight_rule(mut rule: HighlightRule) -> Option<HighlightRule> {
    let color = highlight_color(&rule.color)?;
    rule.color = color.to_string();
    rule.value = match rule.op {
        HighlightOp::IsEmpty | HighlightOp::IsNotEmpty | HighlightOp::IsNull | HighlightOp::IsNotNull => None,
        HighlightOp::Between => Some(FilterValue::Pair(
            match &rule.value {
                Some(FilterValue::Pair(lo, _)) => lo.clone(),
                Some(FilterValue::Single(text)) => text.clone(),
                _ => String::new(),
            },
            match &rule.value {
                Some(FilterValue::Pair(_, hi)) => hi.clone(),
                _ => String::new(),
            },
        )),
        HighlightOp::In | HighlightOp::NotIn => Some(FilterValue::List(match rule.value {
            Some(FilterValue::List(items)) => items,
            Some(FilterValue::Single(text)) if !text.trim().is_empty() => text
                .split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect(),
            _ => Vec::new(),
        })),
        _ => Some(FilterValue::Single(match rule.value {
            Some(FilterValue::Single(text)) => text,
            _ => String::new(),
        })),
    };
    Some(rule)
}

impl HighlightSettingsStore {
    pub fn open() -> Option<Self> {
        Some(Self::load_at(xdg_config_path(FILE)?))
    }

    pub fn load_in(dir: &std::path::Path) -> Self {
        Self::load_at(dir.join(FILE))
    }

    fn load_at(path: std::path::PathBuf) -> Self {
        Self {
            file: Arc::new(StateFile::load(path)),
        }
    }

    pub fn flush(&self) -> Result<(), String> {
        self.file.flush()
    }

    pub fn load(&self, connection_id: Uuid, schema: Option<&str>, table: &str) -> HighlightRuleSet {
        self.file
            .read(|map| {
                map.get(&connection_id.to_string())
                    .and_then(|schemas| schemas.get(&schema_key(schema)))
                    .and_then(|tables| tables.get(table))
                    .cloned()
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    pub fn prepared(
        &self,
        connection_id: Uuid,
        schema: Option<&str>,
        table: &str,
        columns: &[&str],
    ) -> Option<Arc<[PreparedHighlightRule]>> {
        let set = self.load(connection_id, schema, table);
        if set.is_empty() {
            return None;
        }
        let prepared = prepared_rules_for_columns(&set, columns);
        if prepared.is_empty() { None } else { Some(prepared) }
    }

    pub fn save(
        &self,
        connection_id: Uuid,
        schema: Option<&str>,
        table: &str,
        set: HighlightRuleSet,
    ) -> Result<(), String> {
        self.file.update(|map| {
            if set.is_empty() {
                remove_entry(map, connection_id, schema, table);
            } else {
                map.entry(connection_id.to_string())
                    .or_default()
                    .entry(schema_key(schema))
                    .or_default()
                    .insert(table.to_string(), set);
            }
        })
    }
}

fn remove_entry(map: &mut Connections, connection_id: Uuid, schema: Option<&str>, table: &str) {
    let connection = connection_id.to_string();
    let schema = schema_key(schema);
    let Some(schemas) = map.get_mut(&connection) else {
        return;
    };
    let Some(tables) = schemas.get_mut(&schema) else {
        return;
    };
    tables.remove(table);
    if tables.is_empty() {
        schemas.remove(&schema);
    }
    if schemas.is_empty() {
        map.remove(&connection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::FilterValue;
    use bookie_results::highlight_match::{
        DisplayCell, HighlightOp, HighlightPaint, HighlightRule, HighlightTarget, paint_for_cell,
    };

    fn sample_set() -> HighlightRuleSet {
        HighlightRuleSet {
            rules: vec![HighlightRule {
                column: "status".into(),
                op: HighlightOp::Eq,
                value: Some(FilterValue::Single("bad".into())),
                color: "red".into(),
                target: HighlightTarget::Cell,
            }],
        }
    }

    fn mixed_set() -> HighlightRuleSet {
        HighlightRuleSet {
            rules: vec![
                HighlightRule {
                    column: "status".into(),
                    op: HighlightOp::Eq,
                    value: Some(FilterValue::Single("bad".into())),
                    color: "red".into(),
                    target: HighlightTarget::Cell,
                },
                HighlightRule {
                    column: "status".into(),
                    op: HighlightOp::IsEmpty,
                    value: None,
                    color: "chartreuse".into(),
                    target: HighlightTarget::Cell,
                },
                HighlightRule {
                    column: "missing".into(),
                    op: HighlightOp::IsNull,
                    value: None,
                    color: "blue".into(),
                    target: HighlightTarget::Row,
                },
            ],
        }
    }

    fn long_like_rule(pattern: String) -> HighlightRule {
        HighlightRule {
            column: "status".into(),
            op: HighlightOp::Like,
            value: Some(FilterValue::Single(pattern)),
            color: "red".into(),
            target: HighlightTarget::Cell,
        }
    }

    #[test]
    fn pattern_limit_uses_utf8_bytes_and_allows_exact_limit() {
        let original = HighlightRuleSet::default();
        assert!(
            validate_highlight_pattern_lengths(
                &original,
                &HighlightRuleSet {
                    rules: vec![long_like_rule("é".repeat(MAX_LIKE_PATTERN_BYTES / 2))],
                }
            )
            .is_ok()
        );
        assert!(
            validate_highlight_pattern_lengths(
                &original,
                &HighlightRuleSet {
                    rules: vec![long_like_rule("é".repeat(MAX_LIKE_PATTERN_BYTES / 2 + 1))],
                }
            )
            .is_err()
        );
    }

    #[test]
    fn applying_unrelated_changes_preserves_legacy_over_limit_rules() {
        let legacy = HighlightRuleSet {
            rules: vec![long_like_rule("x".repeat(MAX_LIKE_PATTERN_BYTES + 1))],
        };
        let candidate = HighlightRuleSet {
            rules: vec![legacy.rules[0].clone(), default_highlight_rule("kind")],
        };
        assert!(validate_highlight_pattern_lengths(&legacy, &candidate).is_ok());
        assert!(validate_highlight_pattern_lengths(&legacy, &HighlightRuleSet::default()).is_ok());
    }

    #[test]
    fn changed_or_duplicated_legacy_over_limit_rules_are_rejected() {
        let long = "x".repeat(MAX_LIKE_PATTERN_BYTES + 1);
        let legacy = HighlightRuleSet {
            rules: vec![long_like_rule(long.clone())],
        };
        assert!(
            validate_highlight_pattern_lengths(
                &legacy,
                &HighlightRuleSet {
                    rules: vec![long_like_rule(format!("{long}x"))],
                }
            )
            .is_err()
        );
        assert!(
            validate_highlight_pattern_lengths(
                &legacy,
                &HighlightRuleSet {
                    rules: vec![long_like_rule(long.clone()), long_like_rule(long)],
                }
            )
            .is_err()
        );
    }

    #[test]
    fn round_trip_persists_rules_per_table_and_clear_removes_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = HighlightSettingsStore::load_at(dir.path().join(FILE));
        let connection = Uuid::new_v4();
        store
            .save(connection, Some("public"), "orders", sample_set())
            .expect("save");
        store.flush().expect("flush");
        let loaded = store.load(connection, Some("public"), "orders");
        assert_eq!(loaded, sample_set());
        assert!(store.load(connection, Some("public"), "other").is_empty());
        store
            .save(connection, Some("public"), "orders", HighlightRuleSet::default())
            .expect("clear");
        store.flush().expect("flush clear");
        assert!(store.load(connection, Some("public"), "orders").is_empty());
        let raw = std::fs::read_to_string(dir.path().join(FILE)).unwrap_or_default();
        assert!(!raw.contains("orders"), "cleared tables leave no empty blob: {raw}");
    }

    #[test]
    fn normalize_for_save_keeps_palette_rules_and_fixes_value_shapes() {
        let set = normalize_highlight_rules(&[
            HighlightRule {
                column: "status".into(),
                op: HighlightOp::IsNull,
                value: Some(FilterValue::Single("x".into())),
                color: "Red".into(),
                target: HighlightTarget::Cell,
            },
            HighlightRule {
                column: "amount".into(),
                op: HighlightOp::Between,
                value: Some(FilterValue::Single("1".into())),
                color: "blue".into(),
                target: HighlightTarget::Row,
            },
            HighlightRule {
                column: "status".into(),
                op: HighlightOp::Eq,
                value: Some(FilterValue::Single("bad".into())),
                color: "chartreuse".into(),
                target: HighlightTarget::Cell,
            },
        ]);
        assert_eq!(
            set,
            HighlightRuleSet {
                rules: vec![
                    HighlightRule {
                        column: "status".into(),
                        op: HighlightOp::IsNull,
                        value: None,
                        color: "red".into(),
                        target: HighlightTarget::Cell,
                    },
                    HighlightRule {
                        column: "amount".into(),
                        op: HighlightOp::Between,
                        value: Some(FilterValue::Pair("1".into(), String::new())),
                        color: "blue".into(),
                        target: HighlightTarget::Row,
                    },
                ],
            }
        );
        let connection = Uuid::new_v4();
        let dir = tempfile::tempdir().expect("tempdir");
        let store = HighlightSettingsStore::load_in(dir.path());
        let draft = normalize_highlight_rules(&[default_highlight_rule("status")]);
        store
            .save(connection, Some("public"), "orders", draft.clone())
            .expect("save");
        assert_eq!(store.load(connection, Some("public"), "orders"), draft);
    }

    #[test]
    fn prepared_from_store_skips_invalid_rules_and_paints_without_changing_text() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = HighlightSettingsStore::load_at(dir.path().join(FILE));
        let connection = Uuid::new_v4();
        store
            .save(connection, Some("public"), "orders", mixed_set())
            .expect("save");
        let prepared = store
            .prepared(connection, Some("public"), "orders", &["id", "status"])
            .expect("valid rules remain");
        assert_eq!(prepared.len(), 1);
        let shown = "bad";
        let paint = paint_for_cell(
            &prepared,
            1,
            DisplayCell {
                text: shown,
                is_null: false,
            },
            |_| None::<(String, bool)>,
        );
        assert_eq!(
            paint,
            Some(HighlightPaint {
                color: "red",
                target: HighlightTarget::Cell,
            })
        );
        assert_eq!(shown, "bad");
        assert!(store.prepared(connection, Some("public"), "orders", &["id"]).is_none());
        store
            .save(connection, Some("public"), "orders", HighlightRuleSet::default())
            .expect("clear");
        assert!(
            store
                .prepared(connection, Some("public"), "orders", &["status"])
                .is_none()
        );
    }
}
