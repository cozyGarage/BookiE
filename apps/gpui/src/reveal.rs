use std::collections::HashSet;

use bookie_results::sidebar_tree::{Reveal, build_nodes, reveal_target};

use super::*;

impl GridView {
    pub(crate) fn reveal_in_sidebar(&mut self, cx: &mut Context<Self>) {
        let Tab::Table(table) = self.tabs.current().clone() else {
            self.status = "Open a table to reveal it in the sidebar".into();
            return;
        };
        let nodes = build_nodes(&self.catalog.tables, &[]);
        let plan = reveal_target(
            &nodes,
            &self.sidebar.collapsed_schemas,
            "",
            &HashSet::new(),
            table.schema.as_deref(),
            &table.name,
        );
        let Reveal::Show { uncollapse, .. } = plan else {
            self.status = format!("{} is not in the sidebar", table_label(&table));
            return;
        };
        for key in uncollapse {
            self.sidebar.collapsed_schemas.toggle(&key);
        }
        self.sidebar.open = true;
        self.sidebar.connection_open = true;
        self.sidebar.database_open = true;
        self.sidebar.search.update(cx, |input, cx| input.set_content("", cx));
        self.status = format!("Revealed {} in the sidebar", table_label(&table));
    }
}
