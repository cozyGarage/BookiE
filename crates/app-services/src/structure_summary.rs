use bookie_core::{ColumnInfo, ForeignKeyInfo, IndexInfo};

pub fn structure_summary(columns: &[ColumnInfo], indexes: &[IndexInfo], foreign_keys: &[ForeignKeyInfo]) -> String {
    let mut lines = vec![format!("Columns ({})", columns.len())];
    lines.extend(columns.iter().map(column_line));
    lines.push(format!("Indexes ({})", indexes.len()));
    lines.extend(indexes.iter().map(index_line));
    lines.push(format!("Foreign keys ({})", foreign_keys.len()));
    lines.extend(foreign_keys.iter().map(foreign_key_line));
    lines.join("\n")
}

fn column_line(column: &ColumnInfo) -> String {
    let mut notes = Vec::new();
    if column.primary_key {
        notes.push("primary key".to_string());
    }
    if column.is_auto_increment {
        notes.push("auto increment".into());
    }
    if !column.nullable {
        notes.push("not null".into());
    }
    if let Some(default) = &column.default_value {
        notes.push(format!("default {default}"));
    }
    let tail = if notes.is_empty() {
        String::new()
    } else {
        format!(" · {}", notes.join(", "))
    };
    format!("  {} {}{tail}", column.name, column.data_type)
}

fn index_line(index: &IndexInfo) -> String {
    let kind = if index.primary {
        " primary"
    } else if index.unique {
        " unique"
    } else {
        ""
    };
    format!("  {} ({}){kind}", index.name, index.columns.join(", "))
}

fn foreign_key_line(key: &ForeignKeyInfo) -> String {
    let target = match &key.ref_schema {
        Some(schema) => format!("{schema}.{}", key.ref_table),
        None => key.ref_table.clone(),
    };
    let mut line = format!(
        "  {} ({}) → {target} ({})",
        key.name,
        key.columns.join(", "),
        key.ref_columns.join(", ")
    );
    if let Some(rule) = &key.on_delete {
        line.push_str(&format!(" on delete {rule}"));
    }
    if let Some(rule) = &key.on_update {
        line.push_str(&format!(" on update {rule}"));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str, key: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "INTEGER".into(),
            nullable: !key,
            primary_key: key,
            is_auto_increment: key,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }
    }

    #[test]
    fn columns_indexes_and_foreign_keys_read_as_plain_lines() {
        let index = IndexInfo {
            name: "by_owner".into(),
            columns: vec!["owner".into()],
            unique: true,
            primary: false,
            predicate: None,
            constraint: None,
            has_expression_key: false,
            using_clause: None,
        };
        let key = ForeignKeyInfo {
            name: "fk_owner".into(),
            columns: vec!["owner".into()],
            ref_schema: None,
            ref_table: "people".into(),
            ref_columns: vec!["id".into()],
            on_delete: Some("CASCADE".into()),
            on_update: None,
        };
        let text = structure_summary(&[column("id", true), column("owner", false)], &[index], &[key]);
        assert_eq!(
            text,
            "Columns (2)\n  id INTEGER · primary key, auto increment, not null\n  owner INTEGER\n\
             Indexes (1)\n  by_owner (owner) unique\n\
             Foreign keys (1)\n  fk_owner (owner) → people (id) on delete CASCADE"
        );
    }

    #[test]
    fn a_table_without_indexes_or_keys_says_so_with_zero_counts() {
        let text = structure_summary(&[column("id", false)], &[], &[]);
        assert!(text.ends_with("Indexes (0)\nForeign keys (0)"), "{text}");
    }
}
