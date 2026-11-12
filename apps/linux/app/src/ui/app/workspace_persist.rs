use bookie_app_services::workspace::WorkspaceTabs;
use std::time::Duration;

use relm4::ComponentController;
use relm4::adw::prelude::*;
use relm4::gtk::glib;

use uuid::Uuid;

use crate::services::request_generation;
use crate::services::workspace_state::{self, ConnectionWorkspaceState, WorkspaceTabRecord};

use super::types::read_workspace_tab_id;
use super::{App, WorkspaceTab};

pub(super) const PERSIST_DELAY: Duration = Duration::from_millis(500);

impl App {
    pub(super) fn persist_workspace_state(&self) {
        self.replace_persist_timer();
        let generation = self.persist_generation.get();
        self.persist_pending.set(true);
        let pending = self.persist_pending.clone();
        let timeout = self.persist_timeout.clone();
        let current_generation = self.persist_generation.clone();
        let workspace_tabs = self.workspace_tabs.clone();
        let tab_view = self.workspace_tab_view.clone();
        let targets = self.workspace_targets();
        let workspace = self.workspace.clone();
        let collapsed = self.sidebar_collapsed.clone();
        let hidden_schemas = self.sidebar_hidden_schemas.clone();
        let id = glib::timeout_add_local_once(PERSIST_DELAY, move || {
            timeout.borrow_mut().take();
            pending.set(false);
            if !request_generation::is_current(generation, current_generation.get()) {
                return;
            }
            do_persist_workspace_state(
                &workspace,
                &targets,
                &workspace_tabs,
                tab_view.as_ref(),
                collapsed.borrow().keys(),
                sorted_hidden_schemas(&hidden_schemas.borrow()),
            );
        });
        *self.persist_timeout.borrow_mut() = Some(id);
    }

    pub(super) fn cancel_persist_timer(&self) {
        self.replace_persist_timer();
        self.persist_pending.set(false);
    }

    pub(super) fn do_persist_workspace_state_now(&self) {
        self.cancel_persist_timer();
        do_persist_workspace_state(
            &self.workspace,
            &self.workspace_targets(),
            &self.workspace_tabs,
            self.workspace_tab_view.as_ref(),
            self.sidebar_collapsed.borrow().keys(),
            sorted_hidden_schemas(&self.sidebar_hidden_schemas.borrow()),
        );
    }

    pub(super) fn workspace_targets(&self) -> Vec<PersistTarget> {
        let front = self.connection_id;
        self.multi
            .sessions
            .working_order()
            .iter()
            .filter_map(|id| {
                let is_front = Some(*id) == front;
                let (database, token) = if is_front {
                    (self.connected_database.clone()?, self.workspace_save_token.clone()?)
                } else {
                    (
                        self.multi.catalogs.get(id)?.database.clone(),
                        self.workspace.connection_save_token(*id),
                    )
                };
                Some(PersistTarget {
                    connection_id: *id,
                    database,
                    token,
                    tabs: self.multi.sessions.tabs_for_connection(*id).into_iter().collect(),
                    is_front,
                })
            })
            .collect()
    }

    fn replace_persist_timer(&self) {
        if let Some(id) = self.persist_timeout.borrow_mut().take() {
            id.remove();
        }
        self.persist_generation
            .set(self.persist_generation.get().wrapping_add(1));
    }
}

pub(super) struct PersistTarget {
    connection_id: Uuid,
    database: String,
    token: workspace_state::WorkspaceSaveToken,
    tabs: std::collections::HashSet<Uuid>,
    is_front: bool,
}

fn do_persist_workspace_state(
    workspace: &workspace_state::WorkspaceStore,
    targets: &[PersistTarget],
    workspace_tabs: &std::rc::Rc<std::cell::RefCell<WorkspaceTabs<WorkspaceTab>>>,
    tab_view: Option<&relm4::adw::TabView>,
    collapsed_groups: Vec<String>,
    hidden_schemas: Vec<String>,
) {
    for target in targets {
        let (collapsed, hidden) = if target.is_front {
            (collapsed_groups.clone(), hidden_schemas.clone())
        } else {
            workspace
                .load_database(target.connection_id, &target.database)
                .map(|stored| (stored.collapsed_groups, stored.hidden_schemas))
                .unwrap_or_default()
        };
        persist_session_tabs(workspace, target, workspace_tabs, tab_view, collapsed, hidden);
    }
}

fn persist_session_tabs(
    workspace: &workspace_state::WorkspaceStore,
    target: &PersistTarget,
    workspace_tabs: &std::rc::Rc<std::cell::RefCell<WorkspaceTabs<WorkspaceTab>>>,
    tab_view: Option<&relm4::adw::TabView>,
    collapsed_groups: Vec<String>,
    hidden_schemas: Vec<String>,
) {
    let tabs = workspace_tabs.borrow();
    let Some(tab_view) = tab_view else {
        return;
    };
    let pages = tab_view.pages();
    let n = pages.n_items();
    let active_page = tab_view.selected_page();
    let mut tab_records = Vec::with_capacity(n as usize);
    let mut kept = Vec::with_capacity(n as usize);
    let mut active_raw_index = None;
    for i in 0..n {
        let Some(page) = pages.item(i).and_downcast::<relm4::adw::TabPage>() else {
            kept.push(false);
            continue;
        };
        let owned = read_workspace_tab_id(&page).filter(|id| target.tabs.contains(id));
        if owned.is_none() {
            kept.push(false);
            continue;
        }
        if active_page.as_ref() == Some(&page) {
            active_raw_index = Some(i as usize);
        }
        let record = owned
            .and_then(|id| tabs.get(&id).map(|slot| (id, slot)))
            .and_then(|(id, slot)| match slot {
                WorkspaceTab::Editor(s) => Some(WorkspaceTabRecord::Editor {
                    query: s.query.clone(),
                    draft_id: Some(id),
                    file: s.file.as_ref().map(|file| file.path.clone()),
                    mark: super::tab_title::read_tab_mark(&s.page),
                }),
                WorkspaceTab::Structure(_) => None,
                WorkspaceTab::Table(s) if s.table.is_empty() => None,
                WorkspaceTab::Table(s) => Some(table_record(s, tabs.is_preview(&id))),
            });
        kept.push(record.is_some());
        if let Some(record) = record {
            tab_records.push(record);
        }
    }
    let active_idx = active_idx_among_kept(&kept, active_raw_index);
    workspace.save_database_for_session(
        target.connection_id,
        &target.database,
        &target.token,
        ConnectionWorkspaceState {
            tabs: tab_records,
            active_idx,
            collapsed_groups,
            hidden_schemas,
            ..Default::default()
        },
    );
}

fn table_record(slot: &super::TableTabSlot, preview: bool) -> WorkspaceTabRecord {
    let model = slot.browse.model();
    let sort = model.current_sort();
    WorkspaceTabRecord::Table {
        schema: slot.schema.clone(),
        table: slot.table.clone(),
        mode: workspace_state::PersistedTableMode::Data,
        offset: model.current_offset(),
        page_size: model.page_size(),
        sort_col: sort.map(|(column, _)| column),
        sort_asc: sort.map(|(_, ascending)| ascending),
        preview,
        mark: super::tab_title::read_tab_mark(&slot.page),
    }
}

fn sorted_hidden_schemas(hidden_schemas: &std::collections::HashSet<String>) -> Vec<String> {
    let mut schemas: Vec<String> = hidden_schemas.iter().cloned().collect();
    schemas.sort();
    schemas
}

/// `kept[i]` says whether the tab strip's raw position `i` produced a
/// persisted record -- a Structure tab or an empty-table draft never
/// does. Returns the active tab's position within the persisted list,
/// which drifts from its raw position as soon as a skipped tab appears
/// before it. Falls back to 0 when the active tab itself is one of the
/// kinds that's never persisted, since there is nothing truthful to
/// restore it to.
fn active_idx_among_kept(kept: &[bool], active_raw_index: Option<usize>) -> u32 {
    let Some(active_raw_index) = active_raw_index else {
        return 0;
    };
    if !kept.get(active_raw_index).copied().unwrap_or(false) {
        return 0;
    }
    kept[..active_raw_index].iter().filter(|&&k| k).count() as u32
}

#[cfg(test)]
mod tests {
    use super::active_idx_among_kept;

    #[test]
    fn active_idx_matches_raw_index_when_nothing_is_skipped() {
        assert_eq!(active_idx_among_kept(&[true, true, true], Some(2)), 2);
    }

    #[test]
    fn a_skipped_tab_before_the_active_one_shifts_its_index_down() {
        // Structure tab at 0, active Editor tab at raw index 2 -- only
        // one tab (at raw index 1) was actually persisted before it.
        assert_eq!(active_idx_among_kept(&[false, true, true], Some(2)), 1);
    }

    #[test]
    fn multiple_skipped_tabs_before_the_active_one_all_shift_it_down() {
        assert_eq!(active_idx_among_kept(&[false, false, true, true], Some(3)), 1);
    }

    #[test]
    fn an_active_tab_that_is_itself_skipped_falls_back_to_zero() {
        assert_eq!(active_idx_among_kept(&[true, false, true], Some(1)), 0);
    }

    #[test]
    fn no_active_page_falls_back_to_zero() {
        assert_eq!(active_idx_among_kept(&[true, true], None), 0);
    }
}
