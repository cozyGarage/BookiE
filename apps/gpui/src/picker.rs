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
    SavedFilter(String),
    DeleteSavedFilter(String),
    NameFilter(String),
    SchemaContext(Option<String>),
    Theme(String),
}

impl GridView {
    pub(crate) fn picker_items(&self, cx: &App) -> Vec<PickerItem> {
        let needle = self.palette.input.read(cx).content();
        match self.palette.mode {
            PaletteMode::Commands => Vec::new(),
            PaletteMode::Tables => self.favorite_and_table_items(needle),
            PaletteMode::Columns => self.column_items(needle),
            PaletteMode::Shortcuts => self.shortcut_items(needle),
            PaletteMode::Favorites | PaletteMode::DeleteFavorite => self.favorite_items(needle),
            PaletteMode::SavedFilters | PaletteMode::DeleteSavedFilter => self.saved_filter_items(needle),
            PaletteMode::NameFilter => self.name_filter_items(needle, cx),
            PaletteMode::SchemaContext => self.schema_context_items(needle),
            PaletteMode::Themes => theme_items(needle, cx),
        }
    }

    fn favorite_and_table_items(&self, needle: &str) -> Vec<PickerItem> {
        let mut items: Vec<PickerItem> = self
            .favorite_items_labeled(needle, "Favorite", false)
            .into_iter()
            .collect();
        items.extend(self.table_items(needle));
        items
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

    fn saved_filter_items(&self, needle: &str) -> Vec<PickerItem> {
        let delete = self.palette.mode == PaletteMode::DeleteSavedFilter;
        let mut scored: Vec<(u32, usize, PickerItem)> = self
            .search
            .saved
            .names
            .iter()
            .enumerate()
            .filter_map(|(position, name)| {
                let score = bookie_gpui::palette::fuzzy_score(needle, name);
                (score > 0).then(|| {
                    let pick = if delete {
                        Pick::DeleteSavedFilter(name.clone())
                    } else {
                        Pick::SavedFilter(name.clone())
                    };
                    let item = PickerItem {
                        title: name.clone(),
                        subtitle: "Saved filter".into(),
                        pick,
                    };
                    (score, position, item)
                })
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, _, item)| item).collect()
    }

    fn name_filter_items(&self, needle: &str, cx: &App) -> Vec<PickerItem> {
        let name = needle.trim();
        if name.is_empty() {
            return Vec::new();
        }
        vec![PickerItem {
            title: format!("Save as \u{201c}{name}\u{201d}"),
            subtitle: self.current_filter_text(cx),
            pick: Pick::NameFilter(name.to_string()),
        }]
    }

    fn schema_context_items(&self, needle: &str) -> Vec<PickerItem> {
        let Some(session) = &self.live else {
            return Vec::new();
        };
        schema_context_items_for(&session.driver_id, needle)
    }

    fn favorite_items(&self, needle: &str) -> Vec<PickerItem> {
        let delete = self.palette.mode == PaletteMode::DeleteFavorite;
        self.favorite_items_labeled(needle, "", delete)
    }

    fn favorite_items_labeled(&self, needle: &str, label: &str, delete: bool) -> Vec<PickerItem> {
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
                    let subtitle = if label.is_empty() {
                        preview(&favorite.sql)
                    } else {
                        label.to_owned()
                    };
                    let item = PickerItem {
                        title: favorite.name.clone(),
                        subtitle,
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
        self.palette
            .input
            .update(cx, |input, cx| input.set_placeholder(mode.placeholder(), cx));
        if matches!(mode, PaletteMode::Favorites | PaletteMode::DeleteFavorite) {
            self.refresh_favorites(cx);
        }
        if matches!(mode, PaletteMode::SavedFilters | PaletteMode::DeleteSavedFilter) {
            self.refresh_saved_filter_names();
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
            Pick::SavedFilter(name) => self.apply_saved_filter(&name, cx),
            Pick::DeleteSavedFilter(name) => self.delete_saved_filter(&name, cx),
            Pick::NameFilter(name) => self.save_current_filter_as(&name, cx),
            Pick::SchemaContext(name) => {
                self.schema_context = name.clone();
                self.status = name.map_or_else(
                    || "Keeping the session’s current schema or database".into(),
                    |name| format!("Schema or database context set to {name}"),
                );
            }
            Pick::Theme(name) => self.choose_theme(&name, window, cx),
            Pick::Column(column) => {
                let row = self.cell_cursor.map_or(0, |cursor| cursor.row);
                self.place_cursor(Some(Cursor { row, column }));
            }
        }
        cx.notify();
    }
}

fn schema_context_items_for(driver_id: &str, needle: &str) -> Vec<PickerItem> {
    if !bookie_app_services::schema_context::supports_schema_context(driver_id) {
        return Vec::new();
    }
    let name = needle.trim();
    if name.is_empty() {
        return vec![PickerItem {
            title: "Keep the current session schema or database".into(),
            subtitle: "Do not add a context statement; keep the session’s current context".into(),
            pick: Pick::SchemaContext(None),
        }];
    }
    if bookie_app_services::schema_context::schema_context_statement(driver_id, name).is_err() {
        return Vec::new();
    }
    vec![PickerItem {
        title: format!("Use schema or database “{name}”"),
        subtitle: "Applied before each query on this session".into(),
        pick: Pick::SchemaContext(Some(name.to_owned())),
    }]
}

#[cfg(test)]
mod schema_context_tests {
    use super::{Pick, schema_context_items_for};

    #[test]
    fn picker_offers_schema_context_for_postgres_and_mysql_but_not_sqlite() {
        assert!(schema_context_items_for("postgres", "billing")
            .first()
            .is_some_and(|item| matches!(&item.pick, Pick::SchemaContext(Some(_)))));
        assert!(schema_context_items_for("mysql", "billing")
            .first()
            .is_some_and(|item| matches!(&item.pick, Pick::SchemaContext(Some(_)))));
        assert!(schema_context_items_for("sqlite", "billing").is_empty());
    }

    #[test]
    fn picker_keeps_the_current_session_context_when_the_input_is_empty() {
        assert!(schema_context_items_for("postgres", " ")
            .first()
            .is_some_and(|item| matches!(&item.pick, Pick::SchemaContext(None))));
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

pub(crate) const BUILT_IN_LIGHT: &str = "BookiE Light";
pub(crate) const BUILT_IN_DARK: &str = "BookiE Dark";

fn theme_items(needle: &str, cx: &App) -> Vec<PickerItem> {
    let names = [BUILT_IN_LIGHT.into(), BUILT_IN_DARK.into()]
        .into_iter()
        .chain(crate::ui_theme::bundled_theme_names(cx));
    let mut scored: Vec<(u32, usize, PickerItem)> = names
        .enumerate()
        .filter_map(|(position, name)| {
            let score = bookie_gpui::palette::fuzzy_score(needle, &name);
            (score > 0).then(|| {
                let item = PickerItem {
                    title: name.to_string(),
                    subtitle: "Theme".into(),
                    pick: Pick::Theme(name.to_string()),
                };
                (score, position, item)
            })
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, item)| item).collect()
}

pub(crate) fn install_theme(name: &str, cx: &mut App) -> bool {
    match name {
        BUILT_IN_LIGHT => Theme::light().install(cx),
        BUILT_IN_DARK => Theme::dark().install(cx),
        other => return crate::ui_theme::install_named(other, cx),
    }
    true
}

impl GridView {
    fn choose_theme(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !install_theme(name, cx) {
            self.status = format!("Theme {name} is not available");
            return;
        }
        self.dark_mode = Theme::current(cx).dark;
        self.fonts.settings.theme = name.to_string();
        self.status = match bookie_gpui::font_settings::save(&self.fonts.settings) {
            Ok(()) => format!("Theme: {name}"),
            Err(error) => format!("Theme: {name} (not saved: {error})"),
        };
        window.refresh();
    }
}
