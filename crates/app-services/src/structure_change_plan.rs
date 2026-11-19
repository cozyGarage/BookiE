use bookie_core::sql_ddl::{BuildDdlError, DraftColumn, StructureOp, materialize_ops};
use bookie_core::{ColumnInfo, ForeignKeyInfo, IndexInfo};

use crate::cell_save_plan::SaveTarget;
use crate::change_tracker::EditStatement;
use crate::read_session::TableStructure;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureChangeKind {
    AddColumn,
    RenameColumn,
    DropColumn,
    AddIndex,
    DropIndex,
    AddForeignKey,
    DropForeignKey,
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
            Self::DropIndex => "name of the index to drop",
            Self::AddForeignKey => "name column[, column…] -> other_table column[, column…]",
            Self::DropForeignKey => "name of the foreign key to drop",
            Self::AddIndex => "[unique] index_name column[, column…], for example: unique by_name customer",
        }
    }
}

pub fn plan_structure_change(
    target: SaveTarget<'_>,
    kind: StructureChangeKind,
    text: &str,
    structure: &TableStructure,
) -> Result<StructureChangePlan, StructureChangeError> {
    let op = build_op(&target, kind, text, structure)?;
    let op_name = match &op {
        StructureOp::DropIndex { index_name, .. } => Some(format!("Index {index_name}")),
        StructureOp::DropForeignKey { fk_name, .. } => Some(format!("Foreign key {fk_name}")),
        _ => None,
    };
    let sql = materialize_ops(&[op], target.driver_id).map_err(describe)?;
    let preview = match &op_name {
        Some(label) => format!("{label}\n{}", sql.join("\n")),
        None => sql.join("\n"),
    };
    Ok(StructureChangePlan {
        statements: sql.into_iter().map(|statement| (statement, Vec::new())).collect(),
        preview,
    })
}

fn build_op(
    target: &SaveTarget<'_>,
    kind: StructureChangeKind,
    text: &str,
    structure: &TableStructure,
) -> Result<StructureOp, StructureChangeError> {
    let schema = target.schema.map(str::to_string);
    let table = target.table.to_string();
    match kind {
        StructureChangeKind::AddColumn => add_column(target.columns, schema, table, text),
        StructureChangeKind::RenameColumn => rename_column(target.columns, schema, table, text),
        StructureChangeKind::AddIndex => add_index(target.columns, schema, table, text),
        StructureChangeKind::DropIndex => drop_index(&structure.indexes, schema, table, text),
        StructureChangeKind::AddForeignKey => add_foreign_key(target.columns, schema, table, text),
        StructureChangeKind::DropForeignKey => drop_foreign_key(&structure.foreign_keys, schema, table, text),
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

fn add_index(
    columns: &[ColumnInfo],
    schema: Option<String>,
    table: String,
    text: &str,
) -> Result<StructureOp, StructureChangeError> {
    let mut rest = text.trim();
    let unique = rest
        .split_once(char::is_whitespace)
        .is_some_and(|(word, _)| word.eq_ignore_ascii_case("unique"));
    if unique {
        rest = rest.split_once(char::is_whitespace).map_or("", |(_, tail)| tail.trim());
    }
    let (name, list) = rest
        .split_once(char::is_whitespace)
        .map(|(name, list)| (name.trim(), list.trim()))
        .filter(|(name, list)| !name.is_empty() && !list.is_empty())
        .ok_or_else(|| StructureChangeError::Input("Type the index name and its columns".into()))?;
    let mut named = Vec::new();
    for word in list.split([',', ' ']).filter(|word| !word.is_empty()) {
        named.push(existing(columns, word)?.name.clone());
    }
    Ok(StructureOp::AddIndex {
        schema,
        table,
        index: IndexInfo {
            name: name.to_string(),
            columns: named,
            unique,
            primary: false,
            predicate: None,
            constraint: None,
            has_expression_key: false,
            using_clause: None,
        },
    })
}

fn drop_index(
    indexes: &[IndexInfo],
    schema: Option<String>,
    table: String,
    text: &str,
) -> Result<StructureOp, StructureChangeError> {
    let name = text.trim();
    let exact: Vec<&IndexInfo> = indexes.iter().filter(|index| index.name == name).collect();
    let candidates: Vec<&IndexInfo> = if exact.is_empty() {
        indexes
            .iter()
            .filter(|index| index.name.eq_ignore_ascii_case(name))
            .collect()
    } else {
        exact
    };
    let index = match candidates[..] {
        [one] => one,
        [] => {
            return Err(StructureChangeError::Input(format!(
                "The table has no index named {name}"
            )));
        }
        _ => {
            let names: Vec<&str> = candidates.iter().map(|index| index.name.as_str()).collect();
            return Err(StructureChangeError::Input(format!(
                "Several indexes match {name}: {}. Type the exact name",
                names.join(", ")
            )));
        }
    };
    if index.primary || index.constraint.is_some() {
        return Err(StructureChangeError::Input(format!(
            "{} belongs to a key or constraint; drop the constraint instead",
            index.name
        )));
    }
    Ok(StructureOp::DropIndex {
        schema,
        table,
        index_name: index.name.clone(),
        constraint: None,
    })
}

fn add_foreign_key(
    columns: &[ColumnInfo],
    schema: Option<String>,
    table: String,
    text: &str,
) -> Result<StructureOp, StructureChangeError> {
    let usage = || StructureChangeError::Input("Type: name column[, column…] -> other_table column[, column…]".into());
    let (left, right) = text.split_once("->").ok_or_else(usage)?;
    let (name, own) = left
        .trim()
        .split_once(char::is_whitespace)
        .map(|(name, own)| (name.trim(), own.trim()))
        .filter(|(name, own)| !name.is_empty() && !own.is_empty())
        .ok_or_else(usage)?;
    let (target, theirs) = right
        .trim()
        .split_once(char::is_whitespace)
        .map(|(target, theirs)| (target.trim(), theirs.trim()))
        .filter(|(target, theirs)| !target.is_empty() && !theirs.is_empty())
        .ok_or_else(usage)?;
    let mut own_columns = Vec::new();
    for word in own.split([',', ' ']).filter(|word| !word.is_empty()) {
        own_columns.push(existing(columns, word)?.name.clone());
    }
    let ref_columns: Vec<String> = theirs
        .split([',', ' '])
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect();
    if own_columns.len() != ref_columns.len() {
        return Err(StructureChangeError::Input(
            "The foreign key needs as many columns on the other table as on this one".into(),
        ));
    }
    Ok(StructureOp::AddForeignKey {
        schema: schema.clone(),
        table,
        fk: ForeignKeyInfo {
            name: name.to_string(),
            columns: own_columns,
            ref_schema: schema,
            ref_table: target.to_string(),
            ref_columns,
            on_delete: None,
            on_update: None,
        },
    })
}

fn drop_foreign_key(
    keys: &[ForeignKeyInfo],
    schema: Option<String>,
    table: String,
    text: &str,
) -> Result<StructureOp, StructureChangeError> {
    let name = text.trim();
    let exact: Vec<&ForeignKeyInfo> = keys.iter().filter(|key| key.name == name).collect();
    let candidates: Vec<&ForeignKeyInfo> = if exact.is_empty() {
        keys.iter().filter(|key| key.name.eq_ignore_ascii_case(name)).collect()
    } else {
        exact
    };
    match candidates[..] {
        [one] => Ok(StructureOp::DropForeignKey {
            schema,
            table,
            fk_name: one.name.clone(),
        }),
        [] => Err(StructureChangeError::Input(format!(
            "The table has no foreign key named {name}"
        ))),
        _ => Err(StructureChangeError::Input(format!(
            "Several foreign keys match {name}; type the exact name"
        ))),
    }
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

    fn stored_index(name: &str) -> IndexInfo {
        IndexInfo {
            name: name.into(),
            columns: vec!["note".into()],
            unique: false,
            primary: false,
            predicate: None,
            constraint: None,
            has_expression_key: false,
            using_clause: None,
        }
    }

    fn stored_key(name: &str) -> ForeignKeyInfo {
        ForeignKeyInfo {
            name: name.into(),
            columns: vec!["note".into()],
            ref_schema: None,
            ref_table: "people".into(),
            ref_columns: vec!["id".into()],
            on_delete: None,
            on_update: None,
        }
    }

    fn key_index() -> IndexInfo {
        IndexInfo {
            primary: true,
            ..stored_index("PRIMARY")
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
        let structure = TableStructure {
            indexes: vec![stored_index("by_note"), key_index()],
            foreign_keys: vec![stored_key("fk_owner")],
        };
        plan_structure_change(target, kind, text, &structure)
    }

    #[test]
    fn each_kind_becomes_one_reviewable_statement() {
        let add = plan(StructureChangeKind::AddColumn, "flag INTEGER").unwrap();
        assert!(add.preview.contains("ADD COLUMN"), "{}", add.preview);
        let rename = plan(StructureChangeKind::RenameColumn, "note to memo").unwrap();
        assert!(rename.preview.contains("RENAME"), "{}", rename.preview);
        let drop = plan(StructureChangeKind::DropColumn, "note").unwrap();
        assert!(drop.preview.contains("DROP"), "{}", drop.preview);
        let index = plan(StructureChangeKind::AddIndex, "unique by_note NOTE, id").unwrap();
        assert!(index.preview.contains("CREATE UNIQUE INDEX"), "{}", index.preview);
        assert!(index.preview.contains("\"note\""), "{}", index.preview);
        let gone = plan(StructureChangeKind::DropIndex, "BY_NOTE").unwrap();
        assert!(gone.preview.contains("DROP INDEX"), "{}", gone.preview);
        assert!(
            gone.preview.contains("by_note"),
            "the live spelling is used: {}",
            gone.preview
        );
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
            (StructureChangeKind::AddIndex, "by_x"),
            (StructureChangeKind::AddIndex, "by_x ghost"),
            (StructureChangeKind::DropIndex, "ghost"),
            (StructureChangeKind::DropIndex, "primary"),
            (StructureChangeKind::AddForeignKey, "fk note"),
            (StructureChangeKind::AddForeignKey, "fk ghost -> people id"),
            (StructureChangeKind::AddForeignKey, "fk note -> people id, extra"),
            (StructureChangeKind::DropForeignKey, "ghost"),
            (StructureChangeKind::DropIndex, "by_NOTE two"),
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

    #[test]
    fn two_indexes_that_differ_only_by_case_are_never_guessed() {
        let two = [stored_index("Foo"), stored_index("foo")];
        assert!(drop_index(&two, None, "t".into(), "FOO").is_err());
        assert!(
            drop_index(&two, None, "t".into(), "foo").is_ok(),
            "the exact spelling wins"
        );
    }
}
