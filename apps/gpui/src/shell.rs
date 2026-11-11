use std::collections::HashSet;

use bookie_core::TableInfo;
use bookie_results::sidebar_tree::{CollapseState, NodeKind, build_nodes, visible_nodes_with_hidden_schemas};

pub const CONNECTION: &str = "PostgreSQL · sample";
pub const DATABASE: &str = "sample";
pub const DATA_TABLE: &str = "synthetic_rows";
pub const TABLES: [&str; 4] = [DATA_TABLE, "customers", "orders", "audit_log"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catalog {
    pub connection: String,
    pub database: String,
    pub tables: Vec<TableInfo>,
    pub grid_table: TableInfo,
}

impl Catalog {
    pub fn synthetic() -> Catalog {
        Catalog {
            connection: CONNECTION.into(),
            database: DATABASE.into(),
            tables: TABLES.iter().map(|name| table_info(name)).collect(),
            grid_table: table_info(DATA_TABLE),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeRow {
    Connection { open: bool },
    Database { open: bool },
    Table(TableInfo),
}

pub fn tree_rows(search: &str, connection_open: bool, database_open: bool) -> Vec<(usize, TreeRow)> {
    tree_rows_in(&Catalog::synthetic(), search, connection_open, database_open)
}

pub fn tree_rows_in(
    catalog: &Catalog,
    search: &str,
    connection_open: bool,
    database_open: bool,
) -> Vec<(usize, TreeRow)> {
    let searching = !search.trim().is_empty();
    let nodes = build_nodes(&catalog.tables, &[]);
    let shown = visible_nodes_with_hidden_schemas(&nodes, &CollapseState::default(), search, &HashSet::new());
    let tables: Vec<TableInfo> = shown
        .into_iter()
        .filter_map(|index| match &nodes[index].kind {
            NodeKind::Table { schema, name } => Some(TableInfo {
                schema: schema.clone(),
                name: name.clone(),
            }),
            _ => None,
        })
        .collect();
    if searching && tables.is_empty() {
        return Vec::new();
    }
    let (connection_open, database_open) = (connection_open || searching, database_open || searching);
    let mut rows = vec![(0, TreeRow::Connection { open: connection_open })];
    if connection_open {
        rows.push((1, TreeRow::Database { open: database_open }));
        if database_open {
            rows.extend(tables.into_iter().map(|name| (2, TreeRow::Table(name))));
        }
    }
    rows
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tab {
    Home,
    Table(TableInfo),
    Query,
    Settings,
}

impl Tab {
    pub fn title(&self) -> String {
        match self {
            Tab::Home => "Home".into(),
            Tab::Table(table) => table_label(table),
            Tab::Query => "Query 1".into(),
            Tab::Settings => "Settings".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tabs {
    pub items: Vec<Tab>,
    pub active: usize,
}

impl Default for Tabs {
    fn default() -> Self {
        Tabs {
            items: vec![Tab::Home, Tab::Table(table_info(DATA_TABLE))],
            active: 1,
        }
    }
}

impl Tabs {
    pub fn current(&self) -> &Tab {
        &self.items[self.active]
    }

    pub fn open_table(&mut self, table: &TableInfo) {
        let tab = Tab::Table(table.clone());
        self.active = match self.items.iter().position(|item| *item == tab) {
            Some(index) => index,
            None => {
                self.items.push(tab);
                self.items.len() - 1
            }
        };
    }

    pub fn open_query(&mut self) {
        self.active = match self.items.iter().position(|item| *item == Tab::Query) {
            Some(index) => index,
            None => {
                self.items.push(Tab::Query);
                self.items.len() - 1
            }
        };
    }

    pub fn open_settings(&mut self) {
        self.active = match self.items.iter().position(|item| *item == Tab::Settings) {
            Some(index) => index,
            None => {
                self.items.push(Tab::Settings);
                self.items.len() - 1
            }
        };
    }

    pub fn open_home(&mut self) {
        self.active = 0;
    }

    pub fn close(&mut self, index: usize) {
        if index == 0 || index >= self.items.len() {
            return;
        }
        self.items.remove(index);
        if self.active > index || self.active >= self.items.len() {
            self.active = self.active.saturating_sub(1);
        }
    }

    pub fn shows_grid(&self) -> bool {
        self.shows_grid_for(&table_info(DATA_TABLE))
    }

    pub fn shows_grid_for(&self, grid_table: &TableInfo) -> bool {
        matches!(self.current(), Tab::Query) || *self.current() == Tab::Table(grid_table.clone())
    }
}

pub fn table_info(name: &str) -> TableInfo {
    TableInfo {
        schema: None,
        name: name.into(),
    }
}

pub fn table_label(table: &TableInfo) -> String {
    let name = display_identifier(&table.name);
    table.schema.as_deref().map_or(name.clone(), |schema| {
        format!("{}.{}", display_identifier(schema), name)
    })
}

fn display_identifier(identifier: &str) -> String {
    if !identifier.is_empty()
        && identifier
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '$'))
    {
        return identifier.into();
    }
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResultView {
    #[default]
    Table,
    Summary,
    Messages,
}

pub const MIN_RESULT_RATIO: f32 = 0.2;
pub const MAX_RESULT_RATIO: f32 = 0.8;
pub const DEFAULT_RESULT_RATIO: f32 = 0.68;
pub const RUN_ROWS: [u64; 4] = [150_000, 1_000_000, 5_000, 40];

pub fn clamp_ratio(ratio: f32) -> f32 {
    ratio.clamp(MIN_RESULT_RATIO, MAX_RESULT_RATIO)
}

pub fn result_ratio_from_drag(pointer_y: f32, top: f32, height: f32) -> f32 {
    if height <= 0.0 {
        return DEFAULT_RESULT_RATIO;
    }
    clamp_ratio(1.0 - (pointer_y - top) / height)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub number: usize,
    pub rows: u64,
    pub elapsed_ms: Option<u64>,
}

impl Run {
    pub fn label(self) -> String {
        format!("Run {}", self.number)
    }

    pub fn millis(self) -> u64 {
        self.elapsed_ms.unwrap_or(self.rows / 1_000 + 3)
    }

    pub fn measured(self) -> Option<String> {
        self.elapsed_ms
            .map(|ms| crate::messages::format_elapsed(std::time::Duration::from_millis(ms)))
    }
}

pub fn next_run(number: usize) -> Run {
    Run {
        number,
        rows: RUN_ROWS[(number - 1) % RUN_ROWS.len()],
        elapsed_ms: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(rows: &[(usize, TreeRow)]) -> Vec<String> {
        rows.iter()
            .map(|(depth, row)| {
                let name = match row {
                    TreeRow::Connection { .. } => "conn".to_string(),
                    TreeRow::Database { .. } => "db".to_string(),
                    TreeRow::Table(table) => table_label(table),
                };
                format!("{depth}:{name}")
            })
            .collect()
    }

    #[test]
    fn a_live_catalog_lists_its_own_connection_database_and_tables() {
        let catalog = Catalog {
            connection: "sqlite · shop.db".into(),
            database: "shop.db".into(),
            tables: vec![
                table_info("customers"),
                TableInfo {
                    schema: Some("main".into()),
                    name: "orders".into(),
                },
            ],
            grid_table: table_info("orders"),
        };
        let rows = tree_rows_in(&catalog, "", true, true);
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows[3].1,
            TreeRow::Table(TableInfo {
                schema: Some("main".into()),
                name: "orders".into(),
            })
        );
        assert_eq!(tree_rows_in(&catalog, "cust", false, false).len(), 3);
    }

    #[test]
    fn duplicate_table_names_keep_their_schema_identity_in_tree_and_tabs() {
        let first = TableInfo {
            schema: Some("sales".into()),
            name: "orders".into(),
        };
        let second = TableInfo {
            schema: Some("archive".into()),
            name: "orders".into(),
        };
        let catalog = Catalog {
            connection: "postgres · shop".into(),
            database: "shop".into(),
            tables: vec![first.clone(), second.clone()],
            grid_table: first.clone(),
        };
        let rows = tree_rows_in(&catalog, "orders", true, true);
        assert!(rows.iter().any(|(_, row)| *row == TreeRow::Table(first.clone())));
        assert!(rows.iter().any(|(_, row)| *row == TreeRow::Table(second.clone())));
        let mut tabs = Tabs::default();
        tabs.open_table(&first);
        tabs.open_table(&second);
        assert_eq!(tabs.items.len(), 4);
        assert_eq!(tabs.current(), &Tab::Table(second));
    }

    #[test]
    fn table_labels_quote_dots_and_quotes_without_flattening_identity() {
        let table = TableInfo {
            schema: Some("schema.with\"quote".into()),
            name: "table.name".into(),
        };
        assert_eq!(table_label(&table), "\"schema.with\"\"quote\".\"table.name\"");
        assert_eq!(table.schema.as_deref(), Some("schema.with\"quote"));
        assert_eq!(table.name, "table.name");
    }

    #[test]
    fn a_collapsed_tree_shows_only_the_connection() {
        assert_eq!(labels(&tree_rows("", false, false)), ["0:conn"]);
        assert_eq!(labels(&tree_rows("", true, false)), ["0:conn", "1:db"]);
    }

    #[test]
    fn an_expanded_tree_lists_every_table_under_the_database() {
        let rows = tree_rows("", true, true);
        assert_eq!(
            labels(&rows),
            [
                "0:conn",
                "1:db",
                "2:synthetic_rows",
                "2:customers",
                "2:orders",
                "2:audit_log"
            ]
        );
    }

    #[test]
    fn searching_expands_the_tree_and_keeps_only_matching_tables() {
        assert_eq!(labels(&tree_rows("  OR", false, false)), ["0:conn", "1:db", "2:orders"]);
        assert!(
            tree_rows("zzz", true, true).is_empty(),
            "no match hides the connection too"
        );
    }

    #[test]
    fn opening_a_table_adds_one_tab_and_reopening_reuses_it() {
        let mut tabs = Tabs::default();
        assert!(tabs.shows_grid());
        let orders = table_info("orders");
        tabs.open_table(&orders);
        assert_eq!(tabs.items.len(), 3);
        assert_eq!(tabs.current(), &Tab::Table(orders.clone()));
        tabs.open_table(&orders);
        assert_eq!(tabs.items.len(), 3, "the same table is not opened twice");
        tabs.open_table(&table_info(DATA_TABLE));
        assert!(tabs.shows_grid());
        assert_eq!(tabs.items.len(), 3);
    }

    #[test]
    fn closing_a_tab_keeps_a_valid_active_tab_and_never_closes_home() {
        let mut tabs = Tabs::default();
        tabs.open_table(&table_info("orders"));
        tabs.close(0);
        assert_eq!(tabs.items.len(), 3, "Home cannot be closed");
        tabs.close(2);
        assert_eq!((tabs.items.len(), tabs.active), (2, 1));
        tabs.close(1);
        assert_eq!((tabs.items.len(), tabs.active), (1, 0));
        assert_eq!(tabs.current(), &Tab::Home);
        tabs.close(9);
        assert_eq!(tabs.items.len(), 1);
    }

    #[test]
    fn a_query_tab_opens_once_shows_the_grid_and_can_be_closed() {
        let mut tabs = Tabs::default();
        tabs.open_query();
        assert_eq!(tabs.current(), &Tab::Query);
        assert!(tabs.shows_grid());
        tabs.open_query();
        assert_eq!(tabs.items.len(), 3, "the query tab is reused");
        let orders = table_info("orders");
        tabs.open_table(&orders);
        assert!(!tabs.shows_grid());
        tabs.close(2);
        assert_eq!(tabs.current(), &Tab::Table(orders));
    }

    #[test]
    fn dragging_the_divider_gives_the_result_the_space_below_the_pointer() {
        assert_eq!(
            result_ratio_from_drag(172.0, 72.0, 500.0),
            0.8,
            "100 px below the top: clamped to the maximum"
        );
        assert!((result_ratio_from_drag(322.0, 72.0, 500.0) - 0.5).abs() < 1e-6);
        assert_eq!(result_ratio_from_drag(560.0, 72.0, 500.0), MIN_RESULT_RATIO);
        assert_eq!(result_ratio_from_drag(0.0, 0.0, 0.0), DEFAULT_RESULT_RATIO);
    }

    #[test]
    fn each_run_gets_the_next_number_and_a_known_row_count() {
        let mut runs = Vec::new();
        for _ in 0..5 {
            let run = next_run(runs.len() + 1);
            runs.push(run);
        }
        assert_eq!(runs.iter().map(|run| run.number).collect::<Vec<_>>(), [1, 2, 3, 4, 5]);
        assert_eq!(
            runs.iter().map(|run| run.rows).collect::<Vec<_>>(),
            [150_000, 1_000_000, 5_000, 40, 150_000]
        );
        assert_eq!(runs[0].label(), "Run 1");
        assert_eq!(runs[1].millis(), 1_003);
        assert_eq!(runs[1].measured(), None, "a synthetic run has no measured time");
        let measured = Run {
            elapsed_ms: Some(12),
            ..runs[1]
        };
        assert_eq!((measured.millis(), measured.measured().as_deref()), (12, Some("12 ms")));
    }
}
