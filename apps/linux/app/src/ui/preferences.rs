use relm4::adw::prelude::*;
use relm4::gtk::gio;
use relm4::{adw, gtk};

use crate::services::preferences::{
    ColorSchemePref, DateFormatPref, NullDisplayPref, Preferences, PreferencesStore, ThemePref,
};
use bookie_app_services::preferences::{
    MAX_GRID_TYPOGRAPHY_SIZE, MAX_PAGE_SIZE, MAX_TYPOGRAPHY_SIZE, MIN_PAGE_SIZE, TypographyOverride,
};

struct BrowseGroup {
    group: adw::PreferencesGroup,
    page_size: adw::SpinRow,
    confirm: adw::SwitchRow,
    preview_tab: adw::SwitchRow,
}

struct EditorPage {
    page: adw::PreferencesPage,
    font_size: adw::SpinRow,
    font_family: adw::EntryRow,
    font_weight: adw::ComboRow,
    vim: adw::SwitchRow,
    timeout: adw::SpinRow,
}

struct AppearancePage {
    page: adw::PreferencesPage,
    ui: SurfaceTypographyRows,
    grid: SurfaceTypographyRows,
    output: SurfaceTypographyRows,
}

#[derive(Clone)]
struct SurfaceTypographyRows {
    group: adw::PreferencesGroup,
    family: adw::EntryRow,
    size: adw::SpinRow,
    weight: adw::ComboRow,
}

struct LiveSaveRows {
    page_size: adw::SpinRow,
    confirm: adw::SwitchRow,
    preview_tab: adw::SwitchRow,
    font_size: adw::SpinRow,
    font_family: adw::EntryRow,
    font_weight: adw::ComboRow,
    ui_typography: SurfaceTypographyRows,
    grid_typography: SurfaceTypographyRows,
    output_typography: SurfaceTypographyRows,
    vim: adw::SwitchRow,
    retention: adw::SpinRow,
    timeout: adw::SpinRow,
}

pub fn present(
    parent: &impl IsA<gtk::Widget>,
    history: Option<bookie_storage::query_history::HistoryStore>,
    database: &crate::services::database_service::DatabaseService,
    preferences: &PreferencesStore,
    mcp_bridge: Option<std::sync::Arc<bookie_mcp::McpBridge>>,
) {
    let window = adw::PreferencesDialog::builder()
        .title(bookie_app_services::tr!("Preferences"))
        .build();

    let general = adw::PreferencesPage::builder()
        .title(bookie_app_services::tr!("General"))
        .icon_name("preferences-system-symbolic")
        .build();

    let current = preferences.load();

    let browse = browse_group(parent, preferences, &current);
    general.add(&browse.group);
    let (history_group, retention) = history_group(&window, history, &current);
    general.add(&history_group);
    let appearance = appearance_page(preferences, &current);
    let editor = editor_page(&current);

    window.add(&general);
    window.add(&appearance.page);
    window.add(&editor.page);
    window.add(&super::mcp_preferences::build_page(database, mcp_bridge));

    wire_live_save(
        preferences,
        LiveSaveRows {
            page_size: browse.page_size,
            confirm: browse.confirm,
            preview_tab: browse.preview_tab,
            font_size: editor.font_size,
            font_family: editor.font_family,
            font_weight: editor.font_weight,
            ui_typography: appearance.ui,
            grid_typography: appearance.grid,
            output_typography: appearance.output,
            vim: editor.vim,
            retention,
            timeout: editor.timeout,
        },
    );

    crate::ui::leak_probe::track("preferences", &window);
    window.present(Some(parent));
}

fn appearance_page(preferences: &PreferencesStore, current: &Preferences) -> AppearancePage {
    let appearance = adw::PreferencesPage::builder()
        .title(bookie_app_services::tr!("Appearance"))
        .icon_name("preferences-desktop-appearance-symbolic")
        .build();
    appearance.add(&appearance_style_group(preferences, current));
    let ui = surface_typography_group(
        bookie_app_services::tr!("Application interface"),
        &current.ui_typography,
        MAX_TYPOGRAPHY_SIZE,
    );
    let grid = surface_typography_group(
        bookie_app_services::tr!("Result grid"),
        &current.grid_typography,
        MAX_GRID_TYPOGRAPHY_SIZE,
    );
    let output = surface_typography_group(
        bookie_app_services::tr!("Error and output text"),
        &current.output_typography,
        MAX_TYPOGRAPHY_SIZE,
    );
    appearance.add(&ui.group);
    appearance.add(&grid.group);
    appearance.add(&output.group);
    AppearancePage {
        page: appearance,
        ui,
        grid,
        output,
    }
}

fn appearance_style_group(preferences: &PreferencesStore, current: &Preferences) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(bookie_app_services::tr!("Appearance"))
        .description(bookie_app_services::tr!("Choose how BookiE looks on this desktop."))
        .build();
    let scheme_row = adw::ComboRow::new();
    scheme_row.set_title(&bookie_app_services::tr!("Appearance"));
    scheme_row.set_subtitle(&bookie_app_services::tr!(
        "Follow the system, or always use the light or dark style"
    ));
    scheme_row.set_model(Some(&gtk::StringList::new(&[
        &bookie_app_services::tr!("System"),
        &bookie_app_services::tr!("Light"),
        &bookie_app_services::tr!("Dark"),
    ])));
    scheme_row.set_selected(current.color_scheme.index());
    scheme_row.connect_selected_notify({
        let preferences = preferences.clone();
        move |row| {
            let scheme = ColorSchemePref::from_index(row.selected());
            adw::StyleManager::default().set_color_scheme(crate::services::preferences::color_scheme_style(scheme));
            if let Err(error) = preferences.update(|prefs| prefs.color_scheme = scheme) {
                tracing::warn!(%error, "preferences: color scheme rejected");
            }
        }
    });

    let theme_row = adw::ComboRow::new();
    theme_row.set_title(&bookie_app_services::tr!("Theme"));
    theme_row.set_subtitle(&bookie_app_services::tr!(
        "Colours for windows, sidebars and accents. Works with light and dark appearance."
    ));
    theme_row.set_model(Some(&gtk::StringList::new(
        &crate::ui::theme::labels()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    )));
    theme_row.set_selected(current.theme.index());
    theme_row.connect_selected_notify({
        let preferences = preferences.clone();
        move |row| {
            let theme = ThemePref::from_index(row.selected());
            crate::ui::theme::apply(theme);
            if let Err(error) = preferences.update(|prefs| prefs.theme = theme) {
                tracing::warn!(%error, "preferences: theme rejected");
            }
        }
    });

    group.add(&scheme_row);
    group.add(&theme_row);
    group
}

fn surface_typography_group(title: String, current: &TypographyOverride, max_size: u32) -> SurfaceTypographyRows {
    let group = adw::PreferencesGroup::builder().title(title).build();
    let family = adw::EntryRow::builder()
        .title(bookie_app_services::tr!("Font family (blank uses system)"))
        .text(current.font_family.as_str())
        .build();
    let size = adw::SpinRow::with_range(0.0, f64::from(max_size), 1.0);
    size.set_title(&bookie_app_services::tr!("Font size"));
    size.set_subtitle(&bookie_app_services::tr!("0 uses the system size"));
    size.set_digits(0);
    size.set_value(f64::from(current.font_size));
    let weight = font_weight_row(current.font_weight);
    group.add(&family);
    group.add(&size);
    group.add(&weight);
    SurfaceTypographyRows {
        group,
        family,
        size,
        weight,
    }
}

fn font_weight_row(weight: u32) -> adw::ComboRow {
    let choices = [
        bookie_app_services::tr!("System"),
        bookie_app_services::tr!("Thin"),
        bookie_app_services::tr!("Extra light"),
        bookie_app_services::tr!("Light"),
        bookie_app_services::tr!("Regular"),
        bookie_app_services::tr!("Medium"),
        bookie_app_services::tr!("Semibold"),
        bookie_app_services::tr!("Bold"),
        bookie_app_services::tr!("Extra bold"),
        bookie_app_services::tr!("Heavy"),
    ];
    let row = adw::ComboRow::builder()
        .title(bookie_app_services::tr!("Font weight"))
        .model(&gtk::StringList::new(
            &choices.iter().map(String::as_str).collect::<Vec<_>>(),
        ))
        .build();
    row.set_selected(font_weight_index(weight));
    row
}

fn font_weight_index(weight: u32) -> u32 {
    [0, 100, 200, 300, 400, 500, 600, 700, 800, 900]
        .iter()
        .position(|candidate| *candidate == weight)
        .unwrap_or(0) as u32
}

fn font_weight_value(index: u32) -> u32 {
    [0, 100, 200, 300, 400, 500, 600, 700, 800, 900]
        .get(index as usize)
        .copied()
        .unwrap_or(0)
}

#[cfg(test)]
mod typography_preferences_tests {
    use super::{font_weight_index, font_weight_value};

    #[test]
    fn font_weight_choices_round_trip_system_and_supported_weights() {
        for weight in [0, 100, 200, 300, 400, 500, 600, 700, 800, 900] {
            assert_eq!(font_weight_value(font_weight_index(weight)), weight);
        }
        assert_eq!(font_weight_value(u32::MAX), 0);
        assert_eq!(font_weight_index(650), 0);
    }
}

fn browse_group(parent: &impl IsA<gtk::Widget>, preferences: &PreferencesStore, current: &Preferences) -> BrowseGroup {
    let group = adw::PreferencesGroup::builder()
        .title(bookie_app_services::tr!("Data browser"))
        .description(bookie_app_services::tr!(
            "Choose how table data is displayed and how destructive actions are confirmed."
        ))
        .build();
    let page_size = page_size_row(current);
    let (confirm, preview_tab) = confirmation_rows(current);
    group.add(&null_display_row(preferences, current));
    group.add(&date_format_row(parent, preferences, current));
    group.add(&invisibles_row(preferences, current));
    group.add(&page_size);
    group.add(&confirm);
    group.add(&preview_tab);
    BrowseGroup {
        group,
        page_size,
        confirm,
        preview_tab,
    }
}

fn page_size_row(current: &Preferences) -> adw::SpinRow {
    let page_size_row = adw::SpinRow::with_range(MIN_PAGE_SIZE as f64, MAX_PAGE_SIZE as f64, 1.0);
    page_size_row.set_title(&bookie_app_services::tr!("Default page size"));
    page_size_row.set_subtitle(&bookie_app_services::tr!(
        "Rows fetched per request when browsing a table, up to 5,000 rows."
    ));
    page_size_row.set_digits(0);
    page_size_row.set_value(current.default_page_size as f64);
    page_size_row
}

fn confirmation_rows(current: &Preferences) -> (adw::SwitchRow, adw::SwitchRow) {
    let confirm_row = adw::SwitchRow::builder()
        .title(bookie_app_services::tr!("Confirm before deleting rows"))
        .subtitle(bookie_app_services::tr!(
            "Show a confirmation dialog before each destructive action"
        ))
        .build();
    confirm_row.set_active(current.confirm_destructive);

    let preview_tab_row = adw::SwitchRow::builder()
        .title(bookie_app_services::tr!("Reuse one tab when opening tables"))
        .subtitle(bookie_app_services::tr!(
            "Opening a table replaces the temporary preview tab. Turn off to give every table its own tab."
        ))
        .active(current.open_tables_in_preview_tab)
        .build();
    (confirm_row, preview_tab_row)
}

fn null_display_row(preferences: &PreferencesStore, current: &Preferences) -> adw::ComboRow {
    let null_row = adw::ComboRow::new();
    null_row.set_title(&bookie_app_services::tr!("NULL display"));
    null_row.set_subtitle(&bookie_app_services::tr!(
        "How a NULL cell is shown. Applies to cells drawn after the change."
    ));
    let null_choices = null_display_choices();
    null_row.set_model(Some(&gtk::StringList::new(&[
        null_choices[0].as_str(),
        null_choices[1].as_str(),
        null_choices[2].as_str(),
    ])));
    null_row.set_selected(current.null_display.index());
    null_row.connect_selected_notify({
        let preferences = preferences.clone();
        move |row| {
            let style = NullDisplayPref::from_index(row.selected());
            crate::ui::grid::set_null_display(style);
            if let Err(error) = preferences.update(|prefs| prefs.null_display = style) {
                tracing::warn!(%error, "preferences: null display rejected");
            }
        }
    });
    null_row
}

fn date_format_row(
    parent: &impl IsA<gtk::Widget>,
    preferences: &PreferencesStore,
    current: &Preferences,
) -> adw::ComboRow {
    let date_row = adw::ComboRow::new();
    date_row.set_title(&bookie_app_services::tr!("Date format"));
    date_row.set_subtitle(&bookie_app_services::tr!(
        "How dates and timestamps appear in the grid. Copy and edit keep the database text."
    ));
    let date_choices = date_format_choices();
    date_row.set_model(Some(&gtk::StringList::new(
        &date_choices.iter().map(String::as_str).collect::<Vec<_>>(),
    )));
    date_row.set_selected(current.date_format.index());
    date_row.connect_selected_notify({
        let preferences = preferences.clone();
        let root = parent.upcast_ref::<gtk::Widget>().clone();
        move |row| {
            let format = DateFormatPref::from_index(row.selected());
            crate::ui::grid::set_date_format(format);
            crate::ui::grid::refresh_date_displays_under(&root);
            if let Err(error) = preferences.update(|prefs| prefs.date_format = format) {
                tracing::warn!(%error, "preferences: date format rejected");
            }
        }
    });
    date_row
}

fn invisibles_row(preferences: &PreferencesStore, current: &Preferences) -> adw::SwitchRow {
    let invisibles_row = adw::SwitchRow::builder()
        .title(bookie_app_services::tr!("Mark spaces, tabs and line breaks"))
        .subtitle(bookie_app_services::tr!(
            "Show whitespace marks in the full-value editor. Copy and save keep the exact text."
        ))
        .active(current.show_invisible_characters)
        .build();
    invisibles_row.connect_active_notify({
        let preferences = preferences.clone();
        move |row| {
            let show = row.is_active();
            crate::ui::grid::full_value_editor::set_show_invisible_characters(show);
            if let Err(error) = preferences.update(|prefs| prefs.show_invisible_characters = show) {
                tracing::warn!(%error, "preferences: show invisible characters rejected");
            }
        }
    });
    invisibles_row
}

fn history_group(
    window: &adw::PreferencesDialog,
    history: Option<bookie_storage::query_history::HistoryStore>,
    current: &Preferences,
) -> (adw::PreferencesGroup, adw::SpinRow) {
    let history_group = adw::PreferencesGroup::builder()
        .title(bookie_app_services::tr!("Query history"))
        .description(bookie_app_services::tr!(
            "Persistent record of every SQL query you run."
        ))
        .build();

    let retention_row = adw::SpinRow::with_range(0.0, 365.0, 1.0);
    retention_row.set_title(&bookie_app_services::tr!("Retention (days)"));
    retention_row.set_subtitle(&bookie_app_services::tr!(
        "0 keeps history forever; pinned entries are never pruned."
    ));
    retention_row.set_value(current.history_retention_days as f64);
    history_group.add(&retention_row);
    history_group.add(&clear_history_row(window, history.clone()));
    history_group.add(&storage_location_row(window, history));
    (history_group, retention_row)
}

fn clear_history_row(
    window: &adw::PreferencesDialog,
    history: Option<bookie_storage::query_history::HistoryStore>,
) -> adw::ActionRow {
    // Trigger button uses `.flat`, NOT `.destructive-action`. GNOME
    // Settings convention: the trigger that opens a destructive
    // confirmation dialog is a regular/flat button — the confirmation
    // dialog itself carries the destructive (red) appearance. Pre-
    // coloring the trigger anticipates an action the user hasn't
    // taken yet.
    //
    // Ellipsis on the label per GTK4 HIG: "Use ellipsis when the
    // action requires more user input or confirmation."
    let clear_button = gtk::Button::builder()
        .label(bookie_app_services::tr!("Clear\u{2026}"))
        .valign(gtk::Align::Center)
        .build();
    clear_button.add_css_class("flat");
    let clear_row = adw::ActionRow::builder()
        .title(bookie_app_services::tr!("Clear query history"))
        .subtitle(bookie_app_services::tr!(
            "Removes every saved query, including pinned ones."
        ))
        .build();
    clear_row.add_suffix(&clear_button);
    let dialog_root = window.downgrade();
    let history_for_clear = history.clone();
    clear_button.connect_clicked(move |_| {
        let alert = adw::AlertDialog::new(
            Some(&bookie_app_services::tr!("Clear all query history?")),
            Some(&bookie_app_services::tr!(
                "This permanently deletes every saved query, including pinned ones."
            )),
        );
        alert.add_response("cancel", &bookie_app_services::tr!("Cancel"));
        alert.add_response("clear", &bookie_app_services::tr!("Clear"));
        alert.set_response_appearance("clear", adw::ResponseAppearance::Destructive);
        alert.set_default_response(Some("cancel"));
        alert.set_close_response("cancel");
        let history = history_for_clear.clone();
        alert.connect_response(None, move |_, response| {
            if response == "clear" {
                let history = history.clone();
                relm4::spawn(async move {
                    if let Some(history) = history
                        && let Err(e) = history.clear_all().await
                    {
                        tracing::warn!(error = %e, "history clear_all failed");
                    }
                });
            }
        });
        if let Some(dialog) = dialog_root.upgrade() {
            alert.present(Some(&dialog));
        }
    });
    clear_row
}

fn storage_location_row(
    window: &adw::PreferencesDialog,
    history: Option<bookie_storage::query_history::HistoryStore>,
) -> adw::ActionRow {
    let storage_button = gtk::Button::builder()
        .label(bookie_app_services::tr!("Show in Files"))
        .valign(gtk::Align::Center)
        .build();
    storage_button.add_css_class("flat");
    let storage_subtitle = history
        .as_ref()
        .map(|store| store.path().display().to_string())
        .unwrap_or_else(|| format!("$XDG_CONFIG_HOME/{}/history.db", crate::config::storage_dir_name()));
    let storage_row = adw::ActionRow::builder()
        .use_markup(false)
        .title(bookie_app_services::tr!("Storage location"))
        .subtitle(&storage_subtitle)
        .build();
    // AdwActionRow ellipsis-truncates long subtitles; the tooltip
    // exposes the full path on hover so the user can verify exactly
    // where their history lives without resorting to the file manager.
    storage_row.set_tooltip_text(Some(&storage_subtitle));
    storage_row.add_suffix(&storage_button);
    let parent_for_launcher = window.downgrade();
    let history_for_launcher = history;
    storage_button.connect_clicked(move |_| {
        let Some(path) = history_for_launcher.as_ref().map(|store| store.path().to_path_buf()) else {
            return;
        };
        let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or(path);
        let file = gio::File::for_path(&parent);
        let launcher = gtk::FileLauncher::new(Some(&file));
        let parent_window = parent_for_launcher
            .upgrade()
            .and_then(|window| window.root())
            .and_then(|r| r.downcast::<gtk::Window>().ok());
        launcher.launch(parent_window.as_ref(), gio::Cancellable::NONE, |_| {});
    });
    storage_row
}

fn editor_page(current: &Preferences) -> EditorPage {
    let editor = adw::PreferencesPage::builder()
        .title(bookie_app_services::tr!("Editor"))
        .icon_name("text-editor-symbolic")
        .build();

    let editor_group = adw::PreferencesGroup::builder()
        .title(bookie_app_services::tr!("SQL editor"))
        .description(bookie_app_services::tr!(
            "Set the editor font, key bindings and query timeout."
        ))
        .build();

    let font_size_row = adw::SpinRow::with_range(0.0, f64::from(MAX_TYPOGRAPHY_SIZE), 1.0);
    font_size_row.set_title(&bookie_app_services::tr!("Editor font size"));
    font_size_row.set_subtitle(&bookie_app_services::tr!("0 uses the system size"));
    font_size_row.set_digits(0);
    font_size_row.set_value(current.editor_font_size as f64);
    editor_group.add(&font_size_row);

    let font_family_row = adw::EntryRow::builder()
        .title(bookie_app_services::tr!("Editor font family (blank uses system)"))
        .text(current.editor_font_family.as_str())
        .build();
    editor_group.add(&font_family_row);
    let font_weight = font_weight_row(current.editor_font_weight);
    editor_group.add(&font_weight);

    let vim_row = adw::SwitchRow::builder()
        .title(bookie_app_services::tr!("Vim keys in the SQL editor"))
        .subtitle(bookie_app_services::tr!("Applies to editors opened after the change."))
        .active(current.editor_vim_mode)
        .build();
    editor_group.add(&vim_row);

    // 0 disables, 1..=3600s allowed range. Subtitle exposes the
    // disable-via-zero contract so power users editing long-running
    // analytical queries can opt out without spelunking the JSON.
    let timeout_row = adw::SpinRow::with_range(0.0, 3600.0, 5.0);
    timeout_row.set_title(&bookie_app_services::tr!("Query timeout (seconds)"));
    timeout_row.set_subtitle(&bookie_app_services::tr!(
        "Cancel long-running queries automatically. Set to 0 to disable."
    ));
    timeout_row.set_value(current.query_timeout_secs as f64);
    editor_group.add(&timeout_row);

    editor.add(&editor_group);
    EditorPage {
        page: editor,
        font_size: font_size_row,
        font_family: font_family_row,
        font_weight,
        vim: vim_row,
        timeout: timeout_row,
    }
}

fn wire_live_save(preferences: &PreferencesStore, rows: LiveSaveRows) {
    // Live save — write on every value change instead of batching to
    // window.connect_closed. GNOME Settings applies its preferences
    // immediately (no Apply button); same model here. The previous
    // close-only save lost edits on a crash between change and close.
    let save_all = save_all_closure(preferences, &rows);
    rows.page_size.connect_value_notify({
        let s = save_all.clone();
        move |_| s()
    });
    rows.font_size.connect_value_notify({
        let s = save_all.clone();
        move |_| s()
    });
    rows.vim.connect_active_notify({
        let s = save_all.clone();
        move |_| s()
    });
    rows.font_family.connect_changed({
        let s = save_all.clone();
        move |_| s()
    });
    rows.font_weight.connect_selected_notify({
        let s = save_all.clone();
        move |_| s()
    });
    for typography in [&rows.ui_typography, &rows.grid_typography, &rows.output_typography] {
        typography.family.connect_changed({
            let s = save_all.clone();
            move |_| s()
        });
        typography.size.connect_value_notify({
            let s = save_all.clone();
            move |_| s()
        });
        typography.weight.connect_selected_notify({
            let s = save_all.clone();
            move |_| s()
        });
    }
    rows.retention.connect_value_notify({
        let s = save_all.clone();
        move |_| s()
    });
    rows.timeout.connect_value_notify({
        let s = save_all.clone();
        move |_| s()
    });
    rows.confirm.connect_active_notify({
        let s = save_all.clone();
        move |_| s()
    });
    rows.preview_tab.connect_active_notify({
        let s = save_all.clone();
        move |_| s()
    });
}

fn save_all_closure(preferences: &PreferencesStore, rows: &LiveSaveRows) -> std::rc::Rc<dyn Fn()> {
    let page_size = rows.page_size.clone();
    let confirm = rows.confirm.clone();
    let preview_tab = rows.preview_tab.clone();
    let font = rows.font_size.clone();
    let family = rows.font_family.clone();
    let editor_weight = rows.font_weight.clone();
    let ui = rows.ui_typography.clone();
    let grid = rows.grid_typography.clone();
    let output = rows.output_typography.clone();
    let vim = rows.vim.clone();
    let retention = rows.retention.clone();
    let timeout = rows.timeout.clone();
    let preferences = preferences.clone();
    std::rc::Rc::new(move || {
        let default_page_size = page_size.value().round() as u64;
        if let Err(error) = preferences.update(|prefs| {
            prefs.default_page_size = default_page_size;
            prefs.confirm_destructive = confirm.is_active();
            prefs.open_tables_in_preview_tab = preview_tab.is_active();
            prefs.editor_font_size = font.value() as u32;
            prefs.editor_font_family = family.text().trim().to_string();
            prefs.editor_font_weight = font_weight_value(editor_weight.selected());
            update_typography(&mut prefs.ui_typography, &ui);
            update_typography(&mut prefs.grid_typography, &grid);
            update_typography(&mut prefs.output_typography, &output);
            prefs.editor_vim_mode = vim.is_active();
            prefs.history_retention_days = retention.value() as u32;
            prefs.query_timeout_secs = timeout.value() as u32;
        }) {
            tracing::warn!(%error, "preferences: update rejected");
        } else {
            let current = preferences.load();
            crate::ui::typography::apply(&current);
            crate::ui::editor::apply_font_preferences(&current);
            crate::ui::grid::apply_typography(&current.grid_typography);
        }
    })
}

fn update_typography(target: &mut TypographyOverride, rows: &SurfaceTypographyRows) {
    target.font_family = rows.family.text().trim().to_string();
    target.font_size = rows.size.value() as u32;
    target.font_weight = font_weight_value(rows.weight.selected());
}

fn null_display_choices() -> [String; 3] {
    [
        bookie_app_services::tr!("<NULL>"),
        bookie_app_services::tr!("\u{2205} (empty set)"),
        bookie_app_services::tr!("(null)"),
    ]
}

fn date_format_choices() -> [String; 6] {
    [
        bookie_app_services::tr!("ISO 8601"),
        bookie_app_services::tr!("ISO date"),
        bookie_app_services::tr!("US long"),
        bookie_app_services::tr!("US short"),
        bookie_app_services::tr!("EU long"),
        bookie_app_services::tr!("EU short"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::grid::{set_null_display, value_to_display_text};
    use bookie_core::Value;

    #[test]
    fn the_standard_null_choice_uses_the_editable_null_marker() {
        set_null_display(NullDisplayPref::Standard);
        assert_eq!(
            null_display_choices()[NullDisplayPref::Standard.index() as usize],
            bookie_app_services::tr!("<NULL>")
        );
        assert_eq!(value_to_display_text(&Value::Null), bookie_app_services::tr!("<NULL>"));
    }

    #[test]
    fn date_format_choices_cover_every_preference_row() {
        assert_eq!(date_format_choices().len(), DateFormatPref::ALL.len());
        for (index, format) in DateFormatPref::ALL.into_iter().enumerate() {
            assert_eq!(DateFormatPref::from_index(index as u32), format);
            assert!(!date_format_choices()[index].is_empty());
        }
    }
}
