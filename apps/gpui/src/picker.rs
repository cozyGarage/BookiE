use bookie_app_services::quick_switcher::{QuickTarget, filter as quick_filter, relation_items};
use bookie_core::TableInfo;
use bookie_gpui::keymap::{Chord, Command};
use bookie_results::grid_cursor::Cursor;

use super::*;
use crate::palette_state::PaletteMode;

pub(crate) struct PickerItem {
    pub title: String,
    pub subtitle: String,
    pub pick: Pick,
}

pub(crate) enum Pick {
    Table(TableInfo),
    Column(usize),
    Command(Command),
    Favorite(String),
    DeleteFavorite(uuid::Uuid, String),
}

impl GridView {
    pub(crate) fn picker_items(&self, cx: &App) -> Vec<PickerItem> {
        let needle = self.palette.input.read(cx).content();
        match self.palette.mode {
            PaletteMode::Commands => Vec::new(),
            PaletteMode::Tables => self.table_items(needle),
            PaletteMode::Columns => self.column_items(needle),
            PaletteMode::Shortcuts => self.shortcut_items(needle),
            PaletteMode::Favorites | PaletteMode::DeleteFavorite => self.favorite_items(needle),
        }
    }

    fn table_items(&self, needle: &str) -> Vec<PickerItem> {
        let items = relation_items(&self.catalog.tables, &[], "Table", "View");
        quick_filter(&items, needle)
            .into_iter()
            .filter_map(|item| {
                let QuickTarget::Relation { schema, name } = item.target else {
                    return None;
                };
                let table = self
                    .catalog
                    .tables
                    .iter()
                    .find(|t| t.schema == schema && t.name == name)?;
                Some(PickerItem {
                    title: item.title,
                    subtitle: item.subtitle,
                    pick: Pick::Table(table.clone()),
                })
            })
            .collect()
    }

    fn column_items(&self, needle: &str) -> Vec<PickerItem> {
        let mut scored: Vec<(u32, usize, PickerItem)> = self
            .source
            .columns()
            .iter()
            .enumerate()
            .filter_map(|(position, column)| {
                let score = bookie_gpui::palette::fuzzy_score(needle, &column.name);
                (score > 0).then(|| {
                    let item = PickerItem {
                        title: column.name.clone(),
                        subtitle: format!("#{}", position + 1),
                        pick: Pick::Column(position),
                    };
                    (score, position, item)
                })
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, _, item)| item).collect()
    }

    fn shortcut_items(&self, needle: &str) -> Vec<PickerItem> {
        let mut scored: Vec<(u32, usize, PickerItem)> = Command::ALL
            .into_iter()
            .enumerate()
            .filter_map(|(position, command)| {
                let score = bookie_gpui::palette::fuzzy_score(needle, command.title());
                (score > 0).then(|| {
                    let keys = self
                        .keymap
                        .binding(command)
                        .map_or_else(|| "unbound".into(), Chord::label);
                    let item = PickerItem {
                        title: command.title().into(),
                        subtitle: keys,
                        pick: Pick::Command(command),
                    };
                    (score, position, item)
                })
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, _, item)| item).collect()
    }

    fn favorite_items(&self, needle: &str) -> Vec<PickerItem> {
        let delete = self.palette.mode == PaletteMode::DeleteFavorite;
        let mut scored: Vec<(u32, usize, PickerItem)> = self
            .favorites
            .list
            .iter()
            .enumerate()
            .filter_map(|(position, favorite)| {
                let score = bookie_gpui::palette::fuzzy_score(needle, &favorite.name);
                (score > 0).then(|| {
                    let pick = if delete {
                        Pick::DeleteFavorite(favorite.id, favorite.name.clone())
                    } else {
                        Pick::Favorite(favorite.sql.clone())
                    };
                    let item = PickerItem {
                        title: favorite.name.clone(),
                        subtitle: preview(&favorite.sql),
                        pick,
                    };
                    (score, position, item)
                })
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, _, item)| item).collect()
    }

    pub(crate) fn palette_len(&self, cx: &App) -> usize {
        match self.palette.mode {
            PaletteMode::Commands => self.palette_shown(cx).len(),
            _ => self.picker_items(cx).len(),
        }
    }

    pub(crate) fn open_picker(&mut self, mode: PaletteMode, window: &mut Window, cx: &mut Context<Self>) {
        self.open_palette(window, cx);
        self.palette.mode = mode;
        if matches!(mode, PaletteMode::Favorites | PaletteMode::DeleteFavorite) {
            self.refresh_favorites(cx);
        }
    }

    pub(crate) fn run_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut items = self.picker_items(cx);
        let chosen = self.palette.index.min(items.len().saturating_sub(1));
        if chosen >= items.len() {
            return;
        }
        let item = items.swap_remove(chosen);
        self.close_palette(window, cx);
        match item.pick {
            Pick::Table(table) => self.open_table_named(&table, window, cx),
            Pick::Command(command) => self.run_command(command, window, cx),
            Pick::Favorite(sql) => self.load_favorite(&sql, window, cx),
            Pick::DeleteFavorite(id, name) => self.delete_favorite(id, name, cx),
            Pick::Column(column) => {
                let row = self.cell_cursor.map_or(0, |cursor| cursor.row);
                self.place_cursor(Some(Cursor { row, column }));
            }
        }
        cx.notify();
    }
}

fn preview(sql: &str) -> String {
    let one_line = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut text: String = one_line.chars().take(40).collect();
    if one_line.chars().count() > 40 {
        text.push('…');
    }
    text
}
