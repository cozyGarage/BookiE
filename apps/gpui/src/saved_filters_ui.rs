use std::path::PathBuf;

use bookie_app_services::named_filter_presets::NamedFilterPresetsStore;
use bookie_app_services::simple_filter::{SimpleOp, simple_filter_parts, simple_filter_set};
use uuid::Uuid;

use super::*;

#[derive(Default)]
pub(crate) struct SavedFilters {
    path: Option<PathBuf>,
    store: Option<NamedFilterPresetsStore>,
    pub names: Vec<String>,
}

impl SavedFilters {
    #[cfg(test)]
    pub(crate) fn at(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            ..Self::default()
        }
    }

    fn store(&mut self) -> Option<&NamedFilterPresetsStore> {
        if self.store.is_none() {
            self.store = match &self.path {
                Some(path) => Some(NamedFilterPresetsStore::load_at(path.clone())),
                None => NamedFilterPresetsStore::open(),
            };
        }
        self.store.as_ref()
    }
}

fn operator_word(op: SimpleOp) -> &'static str {
    match op {
        SimpleOp::Contains => "contains",
        SimpleOp::Eq => "=",
        SimpleOp::NotEq => "!=",
    }
}

fn model_op(op: SimpleOp) -> Op {
    match op {
        SimpleOp::Contains => Op::Contains,
        SimpleOp::Eq => Op::Equals,
        SimpleOp::NotEq => Op::NotEquals,
    }
}

fn simple_op(op: Op) -> SimpleOp {
    match op {
        Op::Contains => SimpleOp::Contains,
        Op::Equals => SimpleOp::Eq,
        Op::NotEquals => SimpleOp::NotEq,
    }
}

impl GridView {
    fn saved_filter_target(&self) -> Option<(Uuid, Option<String>, String)> {
        let (id, _) = self.live.as_ref()?.saved.as_ref()?;
        (!self.grid_is_query).then(|| {
            (
                *id,
                self.catalog.grid_table.schema.clone(),
                self.catalog.grid_table.name.clone(),
            )
        })
    }

    pub(crate) fn saved_filter_enabled(&self) -> bool {
        self.saved_filter_target().is_some()
    }

    pub(crate) fn refresh_saved_filter_names(&mut self) {
        let Some((id, schema, table)) = self.saved_filter_target() else {
            self.search.saved.names.clear();
            return;
        };
        let names = self
            .search
            .saved
            .store()
            .map(|store| store.list(id, schema.as_deref(), &table))
            .unwrap_or_default();
        self.search.saved.names = names;
    }

    pub(crate) fn current_filter_text(&self, cx: &App) -> String {
        self.search.filter.read(cx).content().trim().to_string()
    }

    pub(crate) fn can_save_current_filter(&self, cx: &App) -> bool {
        self.saved_filter_enabled() && parse_predicate(&self.current_filter_text(cx), self.source.columns()).is_ok()
    }

    pub(crate) fn save_current_filter_as(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some((id, schema, table)) = self.saved_filter_target() else {
            self.status = "Open a table of a saved connection to save a filter".into();
            return;
        };
        let text = self.current_filter_text(cx);
        let predicate = match parse_predicate(&text, self.source.columns()) {
            Ok(predicate) => predicate,
            Err(message) => {
                self.status = message;
                return;
            }
        };
        let column = self.source.columns()[predicate.column].name.clone();
        let set = simple_filter_set(&column, simple_op(predicate.op), &predicate.text);
        let saved = self.search.saved.store().map(|store| {
            store
                .save(id, schema.as_deref(), &table, name, set)
                .and_then(|()| store.flush())
        });
        self.status = match saved {
            Some(Ok(())) => format!("Saved the filter \u{201c}{}\u{201d}", name.trim()),
            Some(Err(message)) => format!("Could not save the filter: {message}"),
            None => "Saved filters are unavailable on this system".into(),
        };
        self.refresh_saved_filter_names();
        cx.notify();
    }

    pub(crate) fn apply_saved_filter(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some((id, schema, table)) = self.saved_filter_target() else {
            return;
        };
        let set = self
            .search
            .saved
            .store()
            .and_then(|store| store.load(id, schema.as_deref(), &table, name));
        let Some((column, op, text)) = set.as_ref().and_then(simple_filter_parts) else {
            self.status = format!("The filter \u{201c}{name}\u{201d} uses rules GPUI cannot show yet");
            return;
        };
        let columns = self.source.columns();
        let Some(index) = columns.iter().position(|meta| meta.name == column) else {
            self.status = format!("The filter \u{201c}{name}\u{201d} names the column {column}, which this table does not have");
            return;
        };
        if text.len() > 256 {
            self.status = "The text to match is longer than 256 characters".into();
            return;
        }
        let line = format!("{column} {} {text}", operator_word(op));
        self.search.filter_visible = true;
        self.search.filter.update(cx, |input, cx| input.set_content_quiet(&line, cx));
        let predicate = Predicate {
            column: index,
            op: model_op(op),
            text,
        };
        self.apply_predicate(predicate, &line, cx);
    }

    pub(crate) fn delete_saved_filter(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some((id, schema, table)) = self.saved_filter_target() else {
            return;
        };
        let removed = self.search.saved.store().map(|store| {
            store
                .delete(id, schema.as_deref(), &table, name)
                .and_then(|gone| store.flush().map(|()| gone))
        });
        self.status = match removed {
            Some(Ok(true)) => format!("Deleted the filter \u{201c}{name}\u{201d}"),
            Some(Ok(false)) => "That filter no longer exists".into(),
            Some(Err(message)) => format!("Could not delete the filter: {message}"),
            None => "Saved filters are unavailable on this system".into(),
        };
        self.refresh_saved_filter_names();
        cx.notify();
    }
}
