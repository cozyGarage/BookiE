use bookie_core::ColumnInfo;
use bookie_core::sql_ddl::{BuildDdlError, DraftColumn, StructureOp, materialize_ops};

use crate::cell_save_plan::SaveTarget;
use crate::change_tracker::EditStatement;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureChangeKind {
    AddColumn,
    RenameColumn,
    DropColumn,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructureChangePlan {
    pub statements: Vec<EditStatement>,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StructureChangeError {
    #[error("{0}")]
    Input(String),
    #[error("{0}")]
    Build(String),
}

impl StructureChangeKind {
    pub fn hint(self) -> &'static str {
        match self {
            Self::AddColumn => "name type, for example: note TEXT",
            Self::RenameColumn => "old_name new_name",
            Self::DropColumn => "name of the column to drop",
        }
    }
}

pub fn plan_structure_change(
    target: SaveTarget<'_>,
    kind: StructureChangeKind,
    text: &str,
) -> Result<StructureChangePlan, StructureChangeError> {
    let op = build_op(&target, kind, text)?;
    let sql = materialize_ops(&[op], target.driver_id).map_err(describe)?;
    let preview = sql.join("\n");
    Ok(StructureChangePlan {
        statements: sql.into_iter().map(|statement| (statement, Vec::new())).collect(),
        preview,
    })
}

fn build_op(
    target: &SaveTarget<'_>,
    kind: StructureChangeKind,
    text: &str,
) -> Result<StructureOp, StructureChangeError> {
    let schema = target.schema.map(str::to_string);
    let table = target.table.to_string();
    match kind {
        StructureChangeKind::AddColumn => add_column(target.columns, schema, table, text),
        StructureChangeKind::RenameColumn => rename_column(target.columns, schema, table, text),
        StructureChangeKind::DropColumn => {
            let column = existing(target.columns, text.trim())?;
            Ok(StructureOp::DropColumn {
                schema,
                table,
                column_name: column.name.clone(),
                has_default: column.default_value.is_some(),
            })
        }
    }
}

fn add_column(
    columns: &[ColumnInfo],
    schema: Option<String>,
    table: String,
    text: &str,
) -> Result<StructureOp, StructureChangeError> {
    let (name, data_type) = text
        .trim()
        .split_once(char::is_whitespace)
        .map(|(name, rest)| (name.trim(), rest.trim()))
        .filter(|(name, rest)| !name.is_empty() && !rest.is_empty())
        .ok_or_else(|| StructureChangeError::Input("Type the new column's name and type".into()))?;
    refuse_duplicate(columns, name)?;
    refuse_unplain_type(data_type)?;
    Ok(StructureOp::AddColumn {
        schema,
        table,
        column: DraftColumn {
            original: None,
            name: name.to_string(),
            data_type: data_type.to_string(),
            nullable: true,
            primary_key: false,
            auto_increment: false,
            default_value: None,
            comment: None,
            collation: None,
        },
    })
}

fn rename_column(
    columns: &[ColumnInfo],
    schema: Option<String>,
    table: String,
    text: &str,
) -> Result<StructureOp, StructureChangeError> {
    let mut words: Vec<&str> = text.split_whitespace().collect();
    if words.len() == 3 && words[1].eq_ignore_ascii_case("to") {
        words.remove(1);
    }
    let [old_name, new_name] = words[..] else {
        return Err(StructureChangeError::Input(
            "Type the current name and the new name".into(),
        ));
    };
    let old_name = existing(columns, old_name)?.name.clone();
    refuse_duplicate(columns, new_name)?;
    Ok(StructureOp::RenameColumn {
        schema,
        table,
        old_name,
        new_name: new_name.to_string(),
    })
}

const CLAUSE_WORDS: [&str; 16] = [
    "CHECK",
    "REFERENCES",
    "GENERATED",
    "COLLATE",
    "DEFAULT",
    "PRIMARY",
    "UNIQUE",
    "AS",
    "NOT",
    "NULL",
    "CONSTRAINT",
    "KEY",
    "AUTOINCREMENT",
    "AUTO_INCREMENT",
    "ON",
    "COMMENT",
];

fn refuse_unplain_type(data_type: &str) -> Result<(), StructureChangeError> {
    let plain = data_type
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ' ' | '(' | ')' | ',' | '.'));
    let mut depth = 0i32;
    let mut balanced = true;
    for c in data_type.chars() {
        depth += i32::from(c == '(') - i32::from(c == ')');
        balanced &= depth >= 0;
    }
    let clause = data_type
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|word| CLAUSE_WORDS.iter().any(|reserved| word.eq_ignore_ascii_case(reserved)));
    if plain && balanced && depth == 0 && !clause {
        return Ok(());
    }
    Err(StructureChangeError::Input(
        "Give only a column type such as TEXT, INTEGER or VARCHAR(40); constraints and defaults are not supported here"
            .into(),
    ))
}

fn refuse_duplicate(columns: &[ColumnInfo], name: &str) -> Result<(), StructureChangeError> {
    match find(columns, name) {
        Some(_) => Err(StructureChangeError::Input(format!(
            "The table already has a column named {name}"
        ))),
        None => Ok(()),
    }
}

fn find<'a>(columns: &'a [ColumnInfo], name: &str) -> Option<&'a ColumnInfo> {
    columns
        .iter()
        .find(|column| column.name == name)
        .or_else(|| columns.iter().find(|column| column.name.eq_ignore_ascii_case(name)))
}

fn existing<'a>(columns: &'a [ColumnInfo], name: &str) -> Result<&'a ColumnInfo, StructureChangeError> {
    find(columns, name).ok_or_else(|| StructureChangeError::Input(format!("The table has no column named {name}")))
}

fn describe(error: BuildDdlError) -> StructureChangeError {
    StructureChangeError::Build(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "TEXT".into(),
            nullable: true,
            primary_key: false,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }
    }

    fn plan(kind: StructureChangeKind, text: &str) -> Result<StructureChangePlan, StructureChangeError> {
        let columns = [column("id"), column("note")];
        let target = SaveTarget {
            driver_id: "sqlite",
            schema: None,
            table: "orders",
            columns: &columns,
        };
        plan_structure_change(target, kind, text)
    }

    #[test]
    fn each_kind_becomes_one_reviewable_statement() {
        let add = plan(StructureChangeKind::AddColumn, "flag INTEGER").unwrap();
        assert!(add.preview.contains("ADD COLUMN"), "{}", add.preview);
        let rename = plan(StructureChangeKind::RenameColumn, "note to memo").unwrap();
        assert!(rename.preview.contains("RENAME"), "{}", rename.preview);
        let drop = plan(StructureChangeKind::DropColumn, "note").unwrap();
        assert!(drop.preview.contains("DROP"), "{}", drop.preview);
        assert!(add.statements.iter().all(|(_, values)| values.is_empty()));
    }

    #[test]
    fn unknown_duplicate_and_incomplete_input_is_refused_before_any_sql() {
        for (kind, text) in [
            (StructureChangeKind::AddColumn, "note TEXT"),
            (StructureChangeKind::AddColumn, "justaname"),
            (StructureChangeKind::AddColumn, "Note TEXT"),
            (StructureChangeKind::AddColumn, "x TEXT DEFAULT 'a'"),
            (StructureChangeKind::AddColumn, "x TEXT CHECK (1)"),
            (StructureChangeKind::AddColumn, "x TEXT REFERENCES orders"),
            (StructureChangeKind::AddColumn, "x TEXT DEFAULT 'a\\'' , DROP COLUMN id"),
            (StructureChangeKind::RenameColumn, "ghost memo"),
            (StructureChangeKind::RenameColumn, "note id"),
            (StructureChangeKind::DropColumn, "ghost"),
        ] {
            assert!(
                matches!(plan(kind, text), Err(StructureChangeError::Input(_))),
                "{text}"
            );
        }
    }

    #[test]
    fn a_type_that_smuggles_more_sql_is_refused() {
        assert!(plan(StructureChangeKind::AddColumn, "x TEXT); DROP TABLE orders; --").is_err());
        let quoted = plan(StructureChangeKind::AddColumn, "we\"ird TEXT").unwrap();
        assert!(quoted.preview.contains("\"we\"\"ird\""), "{}", quoted.preview);
    }
}
