use std::collections::{BTreeSet, HashMap};

use tablepro_core::{ColumnInfo, ForeignKeyInfo};

#[derive(Debug, Clone)]
pub struct TableRead {
    pub schema: Option<String>,
    pub name: String,
    pub columns: Vec<ColumnInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SchemaDiagram {
    pub tables: Vec<DiagramTable>,
    pub relations: Vec<DiagramRelation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagramTable {
    pub schema: Option<String>,
    pub name: String,
    pub columns: Vec<DiagramColumn>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagramColumn {
    pub name: String,
    pub type_name: String,
    pub primary_key: bool,
    pub foreign_key: bool,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramRelation {
    pub name: String,
    pub from: usize,
    pub from_columns: Vec<usize>,
    pub to: usize,
    pub to_columns: Vec<usize>,
    pub optional: bool,
    pub unique: bool,
}

type TableKey<'a> = (Option<&'a str>, &'a str);

impl SchemaDiagram {
    pub fn build(mut reads: Vec<TableRead>) -> Self {
        reads.retain(|read| !read.columns.is_empty());
        reads.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.schema.cmp(&b.schema)));
        let index: HashMap<TableKey<'_>, usize> = reads
            .iter()
            .enumerate()
            .map(|(at, read)| ((read.schema.as_deref(), read.name.as_str()), at))
            .collect();
        let mut relations = Vec::new();
        for (from, read) in reads.iter().enumerate() {
            for key in &read.foreign_keys {
                let target_schema = key.ref_schema.as_deref().or(read.schema.as_deref());
                let Some(&to) = index.get(&(target_schema, key.ref_table.as_str())) else {
                    continue;
                };
                relations.push(DiagramRelation {
                    name: key.name.clone(),
                    from,
                    from_columns: positions(&read.columns, &key.columns),
                    to,
                    to_columns: positions(&reads[to].columns, &key.ref_columns),
                    optional: key.columns.iter().any(|name| {
                        read.columns
                            .iter()
                            .any(|column| &column.name == name && column.nullable)
                    }),
                    unique: covers_primary_key(&read.columns, &key.columns),
                });
            }
        }
        let tables = reads.into_iter().map(table).collect();
        Self { tables, relations }
    }

    pub fn neighbours(&self, table: usize) -> (Vec<&str>, Vec<&str>) {
        let names = |indexes: BTreeSet<usize>| -> Vec<&str> {
            let mut names: Vec<&str> = indexes
                .into_iter()
                .filter_map(|at| self.tables.get(at))
                .map(|found| found.name.as_str())
                .collect();
            names.sort_unstable();
            names
        };
        let references = self
            .relations
            .iter()
            .filter(|relation| relation.from == table && relation.to != table)
            .map(|relation| relation.to)
            .collect();
        let referenced_by = self
            .relations
            .iter()
            .filter(|relation| relation.to == table && relation.from != table)
            .map(|relation| relation.from)
            .collect();
        (names(references), names(referenced_by))
    }

    pub fn position_of(&self, schema: Option<&str>, name: &str) -> Option<usize> {
        self.tables
            .iter()
            .position(|found| found.schema.as_deref() == schema && found.name == name)
    }
}

fn table(read: TableRead) -> DiagramTable {
    let keyed: BTreeSet<&str> = read
        .foreign_keys
        .iter()
        .flat_map(|key| key.columns.iter().map(String::as_str))
        .collect();
    let columns = read
        .columns
        .iter()
        .map(|column| DiagramColumn {
            name: column.name.clone(),
            type_name: column.data_type.clone(),
            primary_key: column.primary_key,
            foreign_key: keyed.contains(column.name.as_str()),
            nullable: column.nullable,
        })
        .collect();
    DiagramTable {
        schema: read.schema,
        name: read.name,
        columns,
    }
}

fn positions(columns: &[ColumnInfo], names: &[String]) -> Vec<usize> {
    names
        .iter()
        .filter_map(|name| columns.iter().position(|column| &column.name == name))
        .collect()
}

fn covers_primary_key(columns: &[ColumnInfo], key: &[String]) -> bool {
    let key: BTreeSet<&str> = key.iter().map(String::as_str).collect();
    let primary: BTreeSet<&str> = columns
        .iter()
        .filter(|column| column.primary_key)
        .map(|column| column.name.as_str())
        .collect();
    !primary.is_empty() && primary.is_subset(&key)
}
