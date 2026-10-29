use tablepro_app_services::er_diagram::{DiagramColumn, SchemaDiagram};

pub(super) fn table_description(diagram: &SchemaDiagram, table: usize) -> String {
    let Some(found) = diagram.tables.get(table) else {
        return String::new();
    };
    let count = found.columns.len();
    let mut sentences = vec![crate::trn!("{n} column.", "{n} columns.", count).replace("{n}", &count.to_string())];
    let (references, referenced_by) = diagram.neighbours(table);
    if !references.is_empty() {
        sentences.push(crate::tr!("References {tables}.").replace("{tables}", &references.join(", ")));
    }
    if !referenced_by.is_empty() {
        sentences.push(crate::tr!("Referenced by {tables}.").replace("{tables}", &referenced_by.join(", ")));
    }
    if !found.columns.is_empty() {
        let columns: Vec<String> = found.columns.iter().map(column_text).collect();
        sentences.push(crate::tr!("Columns: {columns}.").replace("{columns}", &columns.join("; ")));
    }
    sentences.join(" ")
}

fn column_text(column: &DiagramColumn) -> String {
    let mut parts = vec![column.name.clone(), column.type_name.clone()];
    if column.primary_key {
        parts.push(crate::tr!("primary key"));
    }
    if column.foreign_key {
        parts.push(crate::tr!("foreign key"));
    }
    if column.nullable {
        parts.push(crate::tr!("optional"));
    }
    parts.retain(|part| !part.is_empty());
    parts.join(", ")
}

pub(super) fn diagram_label(schema: Option<&str>) -> String {
    match schema {
        Some(schema) => crate::tr!("Tables in {schema}").replace("{schema}", schema),
        None => crate::tr!("Tables"),
    }
}

#[cfg(test)]
mod tests {
    use tablepro_app_services::er_diagram::TableRead;
    use tablepro_core::{ColumnInfo, ForeignKeyInfo};

    use super::*;

    fn column(name: &str, primary_key: bool, nullable: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "integer".into(),
            nullable,
            primary_key,
            is_auto_increment: false,
            default_value: None,
            is_generated: false,
            comment: None,
            collation: None,
            enum_type: None,
            domain_type: None,
        }
    }

    fn orders_and_customers() -> SchemaDiagram {
        SchemaDiagram::build(vec![
            TableRead {
                schema: None,
                name: "orders".into(),
                columns: vec![column("id", true, false), column("customer_id", false, true)],
                foreign_keys: vec![ForeignKeyInfo {
                    name: "orders_customer".into(),
                    columns: vec!["customer_id".into()],
                    ref_schema: None,
                    ref_table: "customers".into(),
                    ref_columns: vec!["id".into()],
                    on_delete: None,
                    on_update: None,
                }],
            },
            TableRead {
                schema: None,
                name: "customers".into(),
                columns: vec![column("id", true, false)],
                foreign_keys: Vec::new(),
            },
        ])
    }

    #[test]
    fn a_table_is_described_with_its_columns_and_neighbours() {
        let diagram = orders_and_customers();

        assert_eq!(
            table_description(&diagram, 1),
            "2 columns. References customers. Columns: id, integer, primary key; \
             customer_id, integer, foreign key, optional."
        );
        assert_eq!(
            table_description(&diagram, 0),
            "1 column. Referenced by orders. Columns: id, integer, primary key."
        );
        assert_eq!(table_description(&diagram, 9), "");
    }

    #[test]
    fn the_diagram_is_named_for_its_schema() {
        assert_eq!(diagram_label(Some("public")), "Tables in public");
        assert_eq!(diagram_label(None), "Tables");
    }
}
