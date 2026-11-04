pub const CONNECTION: &str = "PostgreSQL · sample";
pub const DATABASE: &str = "sample";
pub const DATA_TABLE: &str = "synthetic_rows";
pub const TABLES: [&str; 4] = [DATA_TABLE, "customers", "orders", "audit_log"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeRow {
    Connection { open: bool },
    Database { open: bool },
    Table(&'static str),
}

pub fn tree_rows(search: &str, connection_open: bool, database_open: bool) -> Vec<(usize, TreeRow)> {
    let needle = search.trim().to_lowercase();
    let searching = !needle.is_empty();
    let tables: Vec<&'static str> = TABLES
        .iter()
        .copied()
        .filter(|name| !searching || name.contains(&needle))
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
    Table(String),
}

impl Tab {
    pub fn title(&self) -> String {
        match self {
            Tab::Home => "Home".into(),
            Tab::Table(name) => name.clone(),
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
            items: vec![Tab::Home, Tab::Table(DATA_TABLE.into())],
            active: 1,
        }
    }
}

impl Tabs {
    pub fn current(&self) -> &Tab {
        &self.items[self.active]
    }

    pub fn open_table(&mut self, name: &str) {
        let tab = Tab::Table(name.to_string());
        self.active = match self.items.iter().position(|item| *item == tab) {
            Some(index) => index,
            None => {
                self.items.push(tab);
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
        *self.current() == Tab::Table(DATA_TABLE.into())
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
                    TreeRow::Table(name) => (*name).to_string(),
                };
                format!("{depth}:{name}")
            })
            .collect()
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
        tabs.open_table("orders");
        assert_eq!(tabs.items.len(), 3);
        assert_eq!(tabs.current(), &Tab::Table("orders".into()));
        tabs.open_table("orders");
        assert_eq!(tabs.items.len(), 3, "the same table is not opened twice");
        tabs.open_table(DATA_TABLE);
        assert!(tabs.shows_grid());
        assert_eq!(tabs.items.len(), 3);
    }

    #[test]
    fn closing_a_tab_keeps_a_valid_active_tab_and_never_closes_home() {
        let mut tabs = Tabs::default();
        tabs.open_table("orders");
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
}
