use super::{DiagramColumn, DiagramRelation, DiagramTable, SchemaDiagram};

pub fn mermaid(diagram: &SchemaDiagram) -> String {
    let mut out = String::from("erDiagram\n");
    for table in &diagram.tables {
        out.push_str(&format!("    {} {{\n", mermaid_entity(table)));
        for column in &table.columns {
            out.push_str(&format!("        {}\n", mermaid_attribute(column)));
        }
        out.push_str("    }\n");
    }
    for relation in drawable(diagram) {
        let (from, to) = (&diagram.tables[relation.from], &diagram.tables[relation.to]);
        let parent = if relation.optional { "|o" } else { "||" };
        let child = if relation.unique { "o|" } else { "o{" };
        out.push_str(&format!(
            "    {} {parent}--{child} {} : \"{}\"\n",
            mermaid_entity(to),
            mermaid_entity(from),
            plain(&relation.name).replace('"', "'")
        ));
    }
    out
}

pub fn dbml(diagram: &SchemaDiagram) -> String {
    let mut blocks: Vec<String> = diagram.tables.iter().map(dbml_table).collect();
    let refs: Vec<String> = drawable(diagram).map(|relation| dbml_ref(diagram, relation)).collect();
    if !refs.is_empty() {
        blocks.push(refs.concat());
    }
    blocks.join("\n")
}

fn drawable(diagram: &SchemaDiagram) -> impl Iterator<Item = &DiagramRelation> {
    diagram.relations.iter().filter(|relation| {
        let columns_exist = |table: usize, columns: &[usize]| {
            diagram
                .tables
                .get(table)
                .is_some_and(|found| columns.iter().all(|column| *column < found.columns.len()))
        };
        !relation.from_columns.is_empty()
            && relation.from_columns.len() == relation.to_columns.len()
            && columns_exist(relation.from, &relation.from_columns)
            && columns_exist(relation.to, &relation.to_columns)
    })
}

fn plain(text: &str) -> String {
    text.chars()
        .map(|character| if character.is_control() { ' ' } else { character })
        .collect()
}

fn qualified(table: &DiagramTable) -> String {
    match &table.schema {
        Some(schema) => format!("{schema}.{}", table.name),
        None => table.name.clone(),
    }
}

fn mermaid_entity(table: &DiagramTable) -> String {
    format!("\"{}\"", plain(&qualified(table)).replace('"', "'"))
}

fn word(text: &str) -> String {
    let word: String = text
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    word.trim_matches('_').to_string()
}

fn mermaid_attribute(column: &DiagramColumn) -> String {
    let keys: Vec<&str> = [(column.primary_key, "PK"), (column.foreign_key, "FK")]
        .into_iter()
        .filter_map(|(set, label)| set.then_some(label))
        .collect();
    let type_name = Some(word(&column.type_name)).filter(|word| !word.is_empty());
    let name = Some(word(&column.name)).filter(|word| !word.is_empty());
    let mut parts = vec![
        type_name.unwrap_or_else(|| "unknown".into()),
        name.unwrap_or_else(|| "column".into()),
    ];
    if !keys.is_empty() {
        parts.push(keys.join(","));
    }
    parts.join(" ")
}

fn dbml_name(text: &str) -> String {
    let text = plain(text);
    let simple = !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_')
        && !text.starts_with(|character: char| character.is_ascii_digit());
    if simple {
        text
    } else {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn dbml_type(type_name: &str) -> String {
    let text = plain(type_name);
    let simple = !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '_' | '(' | ')' | ','));
    if simple {
        text
    } else {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn dbml_table_name(table: &DiagramTable) -> String {
    match &table.schema {
        Some(schema) => format!("{}.{}", dbml_name(schema), dbml_name(&table.name)),
        None => dbml_name(&table.name),
    }
}

fn dbml_table(table: &DiagramTable) -> String {
    let mut out = format!("Table {} {{\n", dbml_table_name(table));
    for column in &table.columns {
        let mut settings = Vec::new();
        if column.primary_key {
            settings.push("pk");
        } else if !column.nullable {
            settings.push("not null");
        }
        out.push_str(&format!(
            "  {} {}",
            dbml_name(&column.name),
            dbml_type(&column.type_name)
        ));
        if !settings.is_empty() {
            out.push_str(&format!(" [{}]", settings.join(", ")));
        }
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

fn dbml_ref(diagram: &SchemaDiagram, relation: &DiagramRelation) -> String {
    let side = |table: usize, columns: &[usize]| {
        let table = &diagram.tables[table];
        let names: Vec<String> = columns.iter().map(|at| dbml_name(&table.columns[*at].name)).collect();
        match names.as_slice() {
            [only] => format!("{}.{only}", dbml_table_name(table)),
            _ => format!("{}.({})", dbml_table_name(table), names.join(", ")),
        }
    };
    let kind = if relation.unique { "-" } else { ">" };
    format!(
        "Ref {}: {} {kind} {}\n",
        dbml_name(&relation.name),
        side(relation.from, &relation.from_columns),
        side(relation.to, &relation.to_columns)
    )
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
