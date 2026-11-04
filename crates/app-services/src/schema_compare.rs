use bookie_core::ForeignKeyInfo;
use bookie_core::sql_ddl::{
    BuildDdlError, DraftColumn, build_create_table, build_drop_table, diff_to_ops, materialize_ops,
};

use crate::er_diagram::TableRead;

const NOT_COMPARED: &str = "indexes, views, routines, triggers, sequences, grants and table options are not compared";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SchemaCompare {
    pub added: Vec<String>,
    pub dropped: Vec<String>,
    pub altered: Vec<TableChange>,
    pub skipped: Vec<String>,
    pub script: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableChange {
    pub table: String,
    pub changes: Vec<String>,
}

impl SchemaCompare {
    pub fn is_identical(&self) -> bool {
        self.added.is_empty() && self.dropped.is_empty() && self.altered.is_empty()
    }
}

struct Side<'a> {
    driver_id: &'a str,
    current_schema: Option<&'a str>,
    desired_schema: Option<&'a str>,
}

#[derive(Default)]
struct Statements {
    creates: Vec<String>,
    alters: Vec<String>,
    drops: Vec<String>,
}

pub fn compare_schemas(
    driver_id: &str,
    current_schema: Option<&str>,
    current: &[TableRead],
    desired_schema: Option<&str>,
    desired: &[TableRead],
) -> Result<SchemaCompare, BuildDdlError> {
    let side = Side {
        driver_id,
        current_schema,
        desired_schema,
    };
    let mut result = SchemaCompare {
        skipped: vec![NOT_COMPARED.into()],
        ..SchemaCompare::default()
    };
    let mut statements = Statements::default();
    for target in sorted_readable(desired) {
        match current.iter().find(|table| table.name == target.name) {
            None => {
                statements.creates.extend(create(&side, target)?);
                result.added.push(target.name.clone());
            }
            Some(existing) => {
                let Some(alter) = alter(&side, existing, target)? else {
                    continue;
                };
                result.altered.push(TableChange {
                    table: existing.name.clone(),
                    changes: alter.iter().map(|statement| first_line(statement)).collect(),
                });
                statements.alters.extend(alter);
            }
        }
    }
    for table in sorted_readable(current) {
        if desired.iter().any(|other| other.name == table.name) {
            continue;
        }
        statements
            .drops
            .push(build_drop_table(driver_id, current_schema, &table.name, false, false)?);
        result.dropped.push(table.name.clone());
    }
    result.script = script(&statements);
    Ok(result)
}

fn sorted_readable(tables: &[TableRead]) -> Vec<&TableRead> {
    let mut readable: Vec<&TableRead> = tables.iter().filter(|table| !table.columns.is_empty()).collect();
    readable.sort_by(|a, b| a.name.cmp(&b.name));
    readable
}

fn create(side: &Side<'_>, target: &TableRead) -> Result<Vec<String>, BuildDdlError> {
    let columns: Vec<DraftColumn> = target.columns.iter().cloned().map(DraftColumn::from_info).collect();
    let keys = retarget(&target.foreign_keys, side);
    build_create_table(side.driver_id, side.current_schema, &target.name, &columns, &[], &keys)
}

fn alter(side: &Side<'_>, existing: &TableRead, target: &TableRead) -> Result<Option<Vec<String>>, BuildDdlError> {
    let drafts: Vec<DraftColumn> = target
        .columns
        .iter()
        .cloned()
        .map(|column| {
            let original = existing.columns.iter().find(|have| have.name == column.name).cloned();
            DraftColumn {
                original,
                ..DraftColumn::from_info(column)
            }
        })
        .collect();
    let ops = diff_to_ops(
        side.current_schema,
        &existing.name,
        &existing.name,
        &existing.columns,
        &drafts,
        &[],
        &[],
        &existing.foreign_keys,
        &retarget(&target.foreign_keys, side),
    );
    if ops.is_empty() {
        return Ok(None);
    }
    materialize_ops(&ops, side.driver_id).map(Some)
}

fn retarget(keys: &[ForeignKeyInfo], side: &Side<'_>) -> Vec<ForeignKeyInfo> {
    keys.iter()
        .cloned()
        .map(|mut key| {
            if key.ref_schema.as_deref() == side.desired_schema {
                key.ref_schema = side.current_schema.map(str::to_string);
            }
            key
        })
        .collect()
}

fn first_line(statement: &str) -> String {
    statement.lines().next().unwrap_or_default().trim().to_string()
}

fn script(statements: &Statements) -> String {
    let mut out = String::from("-- Schema compare: review before running. Nothing has been applied.\n");
    let sections = [
        ("New tables", &statements.creates),
        ("Changed tables", &statements.alters),
        ("Dropped tables (destructive)", &statements.drops),
    ];
    if sections.iter().all(|(_, lines)| lines.is_empty()) {
        out.push_str("-- The schemas match, so there is nothing to change.\n");
        return out;
    }
    for (title, lines) in sections.into_iter().filter(|(_, lines)| !lines.is_empty()) {
        out.push_str(&format!("\n-- {title}\n"));
        for statement in lines {
            out.push_str(statement.trim_end_matches(';'));
            out.push_str(";\n");
        }
    }
    out
}

#[cfg(test)]
#[path = "schema_compare_tests.rs"]
mod tests;
