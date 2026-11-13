use crate::ui::browse_tab::BrowseTabInput;
use crate::ui::editor::SqlEditorInput;

use super::App;

pub(super) fn focus_active_grid(app: &App) {
    if let Some(id) = app.selected_editor_tab_id() {
        app.dispatch_to_editor(id, SqlEditorInput::FocusResultGrid);
    } else if let Some(id) = app.selected_browse_tab_id() {
        app.dispatch_to_tab(id, BrowseTabInput::FocusGrid);
    }
}

pub(super) fn focus_active_editor(app: &App) {
    if let Some(id) = app.selected_editor_tab_id() {
        app.dispatch_to_editor(id, SqlEditorInput::FocusEditor);
    }
}

pub(super) fn save_active_browse_tab(app: &mut App, sender: relm4::ComponentSender<App>) {
    if let Some(id) = app.selected_editor_tab_id() {
        if app.change_trackers.with_tab_ref(id, |tracker| tracker.has_pending()) == Some(true) {
            app.dispatch_to_editor(id, SqlEditorInput::CommitResultEdits);
        } else {
            app.save_editor_file(id, false, sender);
        }
    } else if let Some(id) = app.selected_browse_tab_id() {
        app.dispatch_to_tab(id, BrowseTabInput::CommitSave);
    }
}

pub(super) fn save_browse_tab_by_id(app: &App, id: uuid::Uuid) {
    app.dispatch_to_tab(id, BrowseTabInput::CommitSave);
}

pub(super) fn undo_active_browse_tab(app: &App) {
    if let Some(id) = app.selected_browse_tab_id() {
        app.dispatch_to_tab(id, BrowseTabInput::Undo);
    }
}

pub(super) fn redo_active_browse_tab(app: &App) {
    if let Some(id) = app.selected_browse_tab_id() {
        app.dispatch_to_tab(id, BrowseTabInput::Redo);
    }
}
