use bookie_core::{FilterSet, ForeignKeyInfo, InboundForeignKeyInfo, Value};

#[derive(Debug)]
pub struct ReferencedRowTarget {
    pub foreign_key: String,
    pub schema: Option<String>,
    pub table: String,
    pub filter: FilterSet,
}

pub fn reference_targets(
    foreign_keys: &[ForeignKeyInfo],
    own_schema: Option<&str>,
    column: &str,
    row: &[(String, Value)],
) -> Result<Vec<ReferencedRowTarget>, String> {
    let candidates: Vec<_> = foreign_keys
        .iter()
        .filter(|candidate| candidate.columns.iter().any(|name| name == column))
        .collect();
    if candidates.is_empty() {
        return Err(crate::tr!("This column is not a foreign key"));
    }
    let (targets, errors): (Vec<_>, Vec<_>) = candidates
        .into_iter()
        .map(|foreign_key| reference_target(foreign_key, own_schema, row))
        .partition(Result::is_ok);
    if targets.is_empty() {
        return Err(errors.into_iter().find_map(Result::err).unwrap_or_default());
    }
    Ok(targets.into_iter().flatten().collect())
}

fn reference_target(
    foreign_key: &ForeignKeyInfo,
    own_schema: Option<&str>,
    row: &[(String, Value)],
) -> Result<ReferencedRowTarget, String> {
    let mut rules = Vec::new();
    for (local, referenced) in foreign_key.columns.iter().zip(&foreign_key.ref_columns) {
        let value = row
            .iter()
            .find(|(name, _)| name == local)
            .map(|(_, value)| value)
            .ok_or_else(|| crate::tr!("A column of this foreign key is hidden"))?;
        if matches!(value, Value::Null) {
            return Err(crate::tr!("This foreign key is empty"));
        }
        rules.push(
            bookie_core::equality_rule(referenced, value)
                .ok_or_else(|| crate::tr!("Values of this type can't be used as a filter"))?,
        );
    }
    Ok(ReferencedRowTarget {
        foreign_key: foreign_key.name.clone(),
        schema: foreign_key.ref_schema.clone().or_else(|| own_schema.map(str::to_owned)),
        table: foreign_key.ref_table.clone(),
        filter: FilterSet {
            rules,
            ..FilterSet::default()
        },
    })
}

pub fn referencing_targets(
    inbound_keys: &[InboundForeignKeyInfo],
    row: &[(String, Value)],
) -> Result<Vec<ReferencedRowTarget>, String> {
    if inbound_keys.is_empty() {
        return Err(crate::tr!("No foreign keys reference this table"));
    }
    let (targets, errors): (Vec<_>, Vec<_>) = inbound_keys
        .iter()
        .map(|key| referencing_target(key, row))
        .partition(Result::is_ok);
    if targets.is_empty() {
        return Err(errors.into_iter().find_map(Result::err).unwrap_or_default());
    }
    Ok(targets.into_iter().flatten().collect())
}

fn referencing_target(key: &InboundForeignKeyInfo, row: &[(String, Value)]) -> Result<ReferencedRowTarget, String> {
    let mut rules = Vec::new();
    for (source_col, target_col) in key.source_columns.iter().zip(&key.target_columns) {
        let value = row
            .iter()
            .find(|(name, _)| name == target_col)
            .map(|(_, value)| value)
            .ok_or_else(|| crate::tr!("A column referenced by this foreign key is hidden"))?;
        if matches!(value, Value::Null) {
            return Err(crate::tr!("A referenced value is empty"));
        }
        rules.push(
            bookie_core::equality_rule(source_col, value)
                .ok_or_else(|| crate::tr!("Values of this type can't be used as a filter"))?,
        );
    }
    Ok(ReferencedRowTarget {
        foreign_key: key.name.clone(),
        schema: key.source_schema.clone(),
        table: key.source_table.clone(),
        filter: FilterSet {
            rules,
            ..FilterSet::default()
        },
    })
}

#[cfg(test)]
#[path = "foreign_key_nav_tests.rs"]
mod tests;
