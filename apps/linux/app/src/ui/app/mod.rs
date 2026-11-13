mod app_helpers;
mod backup;
mod browse;
mod browse_save;
mod bundle;
mod catalog_ddl;
mod connection;
mod database_switcher;
mod editor_files;
mod environment_marker;
mod favorites;
mod grid_close;
mod health_report;
pub(crate) mod import;
mod init_css;
mod init_sidebar;
mod init_window;
mod init_workspace;
mod lifecycle_messages;
mod msg;
mod multi_session;
mod open_request;
mod organization;
mod ran_statements;
mod referenced_row;
mod render;
mod row_ops;
mod schema_index;
mod session_restore;
mod session_teardown;
mod shortcuts;
mod sidebar_catalog;
mod sidebar_layout;
mod sidebar_state;
mod status_pages;
mod structure;
mod tab_session;
mod tab_title;
mod types;
mod value_search;
mod whats_new;
mod window_close_prompt;
mod workspace_chrome;
mod workspace_close;
mod workspace_overview;
mod workspace_persist;
mod workspace_preview;
mod workspace_tabs;

use std::sync::Arc;

use relm4::adw::prelude::*;
use relm4::factory::FactoryVecDeque;
use relm4::gtk::{gio, glib};
use relm4::prelude::*;
use relm4::{Controller, adw, gtk};

use bookie_core::DriverRegistry;
use bookie_storage::{ConnectionOrganizationIndex, SavedConnection};
use uuid::Uuid;

use super::browse_tab::BrowseTabInput;
use super::connect_dialog::ConnectDialog;
use super::connection_row::ConnectionRow;
use super::editor::SqlEditorInput;
use super::history_dialog::HistoryDialog;
use super::welcome_view::{WelcomeView, WelcomeViewInit, WelcomeViewOutput};
use crate::services::database_service::ConnectionHealth;

pub use msg::AppMsg;
use render::qualified_label;
use types::{CLOSED_TABS_CAPACITY, ConnectionSwitch, ConnectionTransition, ExportFormat, StatusKind, SwitchDecision};
pub use types::{ClosedTabDescriptor, EditorTabSlot, OpenMode, StructureTabSlot, TableTabSlot, WorkspaceTab};

#[derive(Clone, Copy, Debug)]
pub(crate) enum SessionTeardownAction {
    CloseTab(Uuid),
    Disconnect,
    ConnectionSwitch,
    WindowClose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateCheckSource {
    Manual,
    Quiet,
}

#[derive(Clone)]
pub struct AppInit {
    pub registry: Arc<DriverRegistry>,
    pub persistence: crate::services::persistence_stores::PersistenceStores,
    pub workspace: crate::services::workspace_state::WorkspaceStore,
    pub history: Option<bookie_storage::query_history::HistoryStore>,
    pub database: Arc<crate::services::database_service::DatabaseService>,
    pub preferences: crate::services::preferences::PreferencesStore,
    pub mcp_bridge: Option<Arc<bookie_mcp::McpBridge>>,
}

use bookie_app_services::workspace::{CloseAfterSave, WorkspaceTabs};

pub struct App {
    change_trackers: crate::services::change_tracker::WorkspaceChangeTrackers,
    structure_trackers: crate::services::structure_tracker::WorkspaceStructureTrackers,
    registry: Arc<DriverRegistry>,
    persistence: crate::services::persistence_stores::PersistenceStores,
    workspace: crate::services::workspace_state::WorkspaceStore,
    history: Option<bookie_storage::query_history::HistoryStore>,
    database: Arc<crate::services::database_service::DatabaseService>,
    window_state: std::rc::Rc<std::cell::RefCell<crate::services::window_state::WindowState>>,
    window_close_state_saved: std::rc::Rc<std::cell::Cell<bool>>,
    window_close_state_pending: std::rc::Rc<std::cell::Cell<bool>>,
    preferences: crate::services::preferences::PreferencesStore,
    mcp_bridge: Option<Arc<bookie_mcp::McpBridge>>,
    window: adw::ApplicationWindow,
    split_view: adw::OverlaySplitView,
    sidebar_toggle: gtk::ToggleButton,
    sidebar_reveal: std::rc::Rc<std::cell::Cell<u32>>,
    window_title: adw::WindowTitle,
    header_bar: adw::HeaderBar,
    environment_badge: gtk::Label,
    sidebar_title: adw::WindowTitle,
    disconnect_action: gio::SimpleAction,
    sidebar_model: crate::ui::sidebar_tree_model::SidebarTreeModel,
    sidebar_selection: gtk::SingleSelection,
    sidebar_list: gtk::ListView,
    sidebar_row_factories: init_sidebar::RowFactories,
    sidebar_expansion_handlers: init_sidebar::ExpansionHandlers,
    sidebar_render_context: std::rc::Rc<std::cell::RefCell<init_sidebar::SidebarRenderContext>>,
    sidebar_nodes: std::rc::Rc<std::cell::RefCell<Vec<crate::services::sidebar_tree::TreeNode>>>,
    sidebar_collapsed: std::rc::Rc<std::cell::RefCell<crate::services::sidebar_tree::CollapseState>>,
    sidebar_hidden_schemas: std::rc::Rc<std::cell::RefCell<std::collections::HashSet<String>>>,
    schema_filter_button: gtk::MenuButton,
    sidebar_fold_button: gtk::Button,
    schema_filter_list: gtk::Box,
    sidebar_views: Vec<bookie_core::TableInfo>,
    sidebar_tables: Vec<bookie_core::TableInfo>,
    sidebar_catalog: std::collections::HashMap<bookie_core::CatalogObjectKind, Vec<bookie_core::CatalogObject>>,
    sidebar_catalog_cancel: std::cell::RefCell<tokio_util::sync::CancellationToken>,
    catalog_generation: std::cell::Cell<u64>,
    catalog_cancel: std::cell::RefCell<tokio_util::sync::CancellationToken>,
    catalog_dialog_cancel: std::cell::RefCell<tokio_util::sync::CancellationToken>,
    database_list_cancel: std::cell::RefCell<tokio_util::sync::CancellationToken>,
    content_holder: adw::ToolbarView,
    history_paned: gtk::Paned,
    toast_overlay: adw::ToastOverlay,
    /// Persistent "Connecting…" toast handle. Held so we can dismiss it
    /// when the connect attempt resolves (success or failure). Native
    /// alternative to a fire-and-forget 2 s toast that disappeared
    /// before the connection actually completed.
    connect_progress_toast: Option<adw::Toast>,
    connect_cancel: Option<tokio_util::sync::CancellationToken>,
    reconnect_banner: adw::Banner,
    update_banner: adw::Banner,
    available_update: Option<bookie_app_services::release_updates::LatestRelease>,
    connections_factory: FactoryVecDeque<ConnectionRow>,
    connections_popover: gtk::Popover,
    databases_button: gtk::MenuButton,
    databases_list: gtk::ListBox,
    pending_database_switch: Option<(Uuid, String)>,
    connected_database: Option<String>,
    workspace_save_token: Option<crate::services::workspace_state::WorkspaceSaveToken>,
    health_state: Option<ConnectionHealth>,
    row_op_spinner: gtk::Spinner,
    read_only_badge: gtk::Label,
    table_search: gtk::SearchEntry,
    /// Outer Stack inside `content_holder` — swaps between `"empty"`
    /// (AdwStatusPage "Select a table") and `"tabs"` (the unified
    /// AdwTabOverview hosting both Browse and Editor sub-components).
    workspace_outer_stack: gtk::Stack,
    /// AdwTabOverview wrapping the unified AdwTabBar + AdwTabView.
    /// Built lazily on connect; torn down on disconnect.
    workspace_root: Option<adw::TabOverview>,
    workspace_tab_view: Option<adw::TabView>,
    workspace_tab_bar: Option<adw::TabBar>,
    read_scopes: std::cell::RefCell<crate::services::read_scopes::ReadScopes>,
    recent_pages: std::cell::RefCell<crate::services::recent_tabs::RecentList<adw::TabPage>>,
    recent_walk: std::cell::RefCell<Option<crate::services::recent_tabs::RecentWalk<adw::TabPage>>>,
    /// Idempotency flag for `ensure_workspace_root`.
    workspace_root_added: std::cell::Cell<bool>,
    overview_front_connection_id: std::rc::Rc<std::cell::Cell<Option<Uuid>>>,
    /// Per-tab state. Each entry is either a Browse or Editor tab.
    /// HashMap for O(1) tab_id lookup; display order is read from
    /// `tab_view.pages()` since HashMap is unordered.
    workspace_tabs: std::rc::Rc<std::cell::RefCell<WorkspaceTabs<WorkspaceTab>>>,
    dialog: Option<Controller<ConnectDialog>>,
    favorites: Vec<bookie_storage::SavedQuery>,
    favorites_order: crate::services::request_generation::LatestApplied,
    /// Tables and their columns for schema-aware editor completion.
    schema_index: std::rc::Rc<std::cell::RefCell<crate::ui::editor::SchemaIndex>>,
    history_dialog: Option<Controller<HistoryDialog>>,
    saved_queries_dialog: Option<Controller<crate::ui::saved_queries_dialog::SavedQueriesDialog>>,
    welcome_view: Controller<WelcomeView>,
    /// Driver id is connection-wide, not per-tab.
    current_driver_id: Option<String>,
    table_names: Vec<String>,
    /// Read-only flag is connection-wide; fanned out to every BrowseTab
    /// when toggled.
    read_only: bool,
    /// Default page size for newly-opened browse tabs (from preferences).
    /// Per-tab page size lives on each BrowseTab.
    default_page_size: u64,
    saved_connections: Vec<SavedConnection>,
    /// Groups, tags and favourite flags for the saved connections,
    /// loaded from the sidecar file beside `connections.json`. Held in
    /// the model so the welcome view can be re-arranged without another
    /// disk read on every keystroke in the filter box.
    connection_organization: ConnectionOrganizationIndex,
    /// A parsed bundle waiting on the preview dialog. Nothing reaches
    /// disk until the user confirms the plan it holds.
    pending_import: Option<Box<bundle::PendingImport>>,
    /// An encrypted bundle waiting on its passphrase.
    pending_bundle: Option<bookie_storage::EncryptedBundle>,
    connected: bool,
    connection_id: Option<Uuid>,
    multi: multi_session::MultiSessionState,
    csv_import_progress: Option<crate::ui::import_dialog::ImportProgress>,
    backup_progress: Option<crate::ui::progress_dialog::CancellableDialog>,
    switch: ConnectionSwitch,
    grid_close: grid_close::GridCloseState,
    close_after_save: std::rc::Rc<std::cell::RefCell<CloseAfterSave>>,
    /// Set when the user picked "Save" on the *window*-close dialog.
    /// While true, the last `SaveCompletedForTab` that empties
    /// `close_after_save` triggers `window.close()`. A `SaveFailed`
    /// while in this state aborts the window-close intent. `Rc<Cell>`
    /// so the close-request handler closure can mutate it from outside
    /// `App::update`.
    close_window_after_save: std::rc::Rc<std::cell::Cell<bool>>,
    force_close: std::rc::Rc<std::cell::Cell<bool>>,
    /// Count of in-flight transaction commits (a tab clicked Save
    /// and is awaiting `execute_in_transaction`). Incremented by
    /// `on_execute_browse_transaction` at dispatch, decremented by
    /// `SaveCompletedForTab` and `SaveFailedForTab`. Window-close
    /// blocks while this is > 0 so an async transaction never commits
    /// after the tab / window has been torn down.
    in_flight_saves: std::rc::Rc<std::cell::Cell<usize>>,
    active_browse_saves: std::collections::HashMap<Uuid, Uuid>,
    session_teardown_ready: std::rc::Rc<std::cell::Cell<bool>>,
    session_teardown_pending: std::rc::Rc<std::cell::Cell<bool>>,
    /// Structure tabs currently mid-DDL-transaction. A second Ctrl+S
    /// while a Save is still in flight would dispatch a parallel
    /// transaction and potentially commit twice; this set lets the
    /// dispatch path short-circuit. Cleared on
    /// `StructureSaveCompleted` / `StructureSaveFailed`.
    structure_saves_in_flight: std::rc::Rc<std::cell::RefCell<std::collections::HashSet<Uuid>>>,
    /// Debounce flag for `persist_workspace_state`. Active tabs fire
    /// `WorkspaceTabsChanged` on every selection / drag-reorder /
    /// page-size change / state-changed event; without coalescing,
    /// each one triggers a load-modify-write of the entire
    /// connections JSON. This flag stays `true` while a 500ms timer
    /// is pending; subsequent persist requests in the window no-op.
    persist_pending: std::rc::Rc<std::cell::Cell<bool>>,
    persist_timeout: std::rc::Rc<std::cell::RefCell<Option<glib::SourceId>>>,
    persist_generation: std::rc::Rc<std::cell::Cell<u64>>,
    /// LIFO stack of recently-closed tab descriptors for Ctrl+Shift+T
    /// reopen. Capped at `CLOSED_TABS_CAPACITY`; the oldest entry is
    /// dropped when a new one is pushed against a full stack. Cleared
    /// on disconnect — descriptors reference the active connection's
    /// tables, so reopening across connections would target the wrong
    /// schema. Editor descriptors round-trip the buffer text;
    /// dirty tabs lose their pending row/DDL edits because the
    /// trackers have already been cleared by the close path.
    closed_tabs_stack: std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<(Uuid, ClosedTabDescriptor)>>>,
    /// Extra ApplicationWindows spawned via New Window. Kept alive so
    /// closing the primary window does not drop them mid-use. Removing an
    /// entry (see `AppMsg::ExtraWindowClosed`) drops its `Controller`,
    /// which is what actually triggers that window's `Component::shutdown`.
    extra_windows: Vec<(usize, Controller<App>)>,
    /// Source for the once-a-second health poll, so `shutdown` can remove
    /// it instead of leaving it firing forever after this window closes.
    poll_health_source: Option<glib::SourceId>,
    /// Source for the hourly history prune, same reasoning.
    history_prune_source: Option<glib::SourceId>,
}

#[relm4::component(pub)]
impl SimpleComponent for App {
    type Init = AppInit;
    type Input = AppMsg;
    type Output = ();

    view! {
        #[name = "window"]
        adw::ApplicationWindow {
            set_title: Some("BookiE"),
            set_default_width: 1200,
            set_default_height: 760,
            set_width_request: 600,
            set_height_request: 400,

            adw::ToolbarView {
                #[name = "header_bar"]
                add_top_bar = &adw::HeaderBar {
                    #[name = "window_title"]
                    #[wrap(Some)]
                    set_title_widget = &adw::WindowTitle {
                        set_title: "BookiE",
                    },

                    // Two distinct affordances → two distinct buttons.
                    // SplitButton would imply they're variants of the
                    // same action, but "new connection" and "open
                    // saved" are semantically different (matches GNOME
                    // Files' "New" + "History" pattern, not the
                    // SplitButton-as-Save-with-format pattern).
                    #[name = "new_connection_button"]
                    pack_start = &gtk::Button {
                        set_icon_name: "list-add-symbolic",
                        set_tooltip_text: Some(bookie_app_services::tr!("New connection").as_str()),
                        connect_clicked => AppMsg::OpenConnect,
                    },

                    #[name = "saved_connections_button"]
                    pack_start = &gtk::MenuButton {
                        set_icon_name: "document-open-symbolic",
                        set_tooltip_text: Some(bookie_app_services::tr!("Open saved connection").as_str()),

                        #[wrap(Some)]
                        #[name = "connections_popover"]
                        set_popover = &gtk::Popover {},
                    },

                    #[name = "sidebar_toggle"]
                    pack_start = &gtk::ToggleButton {
                        set_icon_name: "sidebar-show-symbolic",
                        set_tooltip_text: Some(bookie_app_services::tr!("Tables sidebar").as_str()),
                    },

                    #[name = "databases_button"]
                    pack_start = &gtk::MenuButton {
                        set_icon_name: "network-server-symbolic",
                        set_tooltip_text: Some(bookie_app_services::tr!("Switch database").as_str()),
                        set_visible: false,

                        #[wrap(Some)]
                        #[name = "databases_popover"]
                        set_popover = &gtk::Popover {},
                    },

                    #[name = "environment_badge"]
                    pack_end = &gtk::Label {
                        set_visible: false,
                        set_label: &bookie_app_services::tr!("Production"),
                        set_tooltip_text: Some(bookie_app_services::tr!("This is a production connection. Changes can affect live data.").as_str()),
                        set_margin_end: 6,
                        add_css_class: "error",
                        add_css_class: "caption-heading",
                    },

                    #[name = "read_only_badge"]
                    pack_end = &gtk::Label {
                        set_visible: false,
                        set_label: &bookie_app_services::tr!("Read-only"),
                        set_margin_end: 6,
                        add_css_class: "warning",
                        add_css_class: "caption-heading",
                    },

                    #[name = "row_op_spinner"]
                    pack_end = &gtk::Spinner {
                        set_visible: false,
                        set_margin_end: 6,
                        set_tooltip_text: Some(bookie_app_services::tr!("Saving…").as_str()),
                    },

                    #[name = "primary_menu_button"]
                    pack_end = &gtk::MenuButton {
                        set_icon_name: "open-menu-symbolic",
                        set_tooltip_text: Some(bookie_app_services::tr!("Main menu").as_str()),
                        set_primary: true,
                    },
                },

                #[wrap(Some)]
                #[name = "split_view"]
                set_content = &adw::OverlaySplitView {
                    set_min_sidebar_width: 220.0,
                    set_max_sidebar_width: 280.0,
                    set_show_sidebar: false,

                    // Sidebar wrapped in its own AdwToolbarView so it can
                    // carry a sidebar-local AdwHeaderBar with a search
                    // toggle — same structure GNOME Files uses for its
                    // Places sidebar. Window-decoration buttons live on
                    // the outer (main) header bar already, so we hide
                    // them here.
                    #[wrap(Some)]
                    #[name = "sidebar_root"]
                    set_sidebar = &adw::ToolbarView {
                        #[name = "sidebar_header"]
                        add_top_bar = &adw::HeaderBar {
                            set_show_start_title_buttons: false,
                            set_show_end_title_buttons: false,

                            #[wrap(Some)]
                            #[name = "sidebar_title"]
                            set_title_widget = &adw::WindowTitle {
                                set_title: &bookie_app_services::tr!("Tables"),
                            },

                            pack_end = &gtk::Button {
                                set_icon_name: "view-refresh-symbolic",
                                set_tooltip_text: Some(bookie_app_services::tr!("Refresh tables").as_str()),
                                set_action_name: Some("win.refresh-sidebar"),
                            },

                            #[name = "table_search_toggle"]
                            pack_end = &gtk::ToggleButton {
                                set_icon_name: "system-search-symbolic",
                                set_tooltip_text: Some(bookie_app_services::tr!("Search tables").as_str()),
                            },

                            #[name = "sidebar_fold_button"]
                            pack_end = &gtk::Button {
                                set_icon_name: "go-up-symbolic",
                                set_tooltip_text: Some(bookie_app_services::tr!("Collapse all").as_str()),
                                set_visible: false,
                                connect_clicked => AppMsg::ToggleAllSidebarGroups,
                            },

                            #[name = "schema_filter_button"]
                            pack_end = &gtk::MenuButton {
                                set_icon_name: crate::ui::icons::SCHEMA_FILTER_ICON_NAME,
                                set_tooltip_text: Some(bookie_app_services::tr!("Filter schemas").as_str()),
                                set_visible: false,

                                #[wrap(Some)]
                                #[name = "schema_filter_popover"]
                                set_popover = &gtk::Popover {},
                            },
                        },

                        #[wrap(Some)]
                        set_content = &gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,

                            #[name = "table_search_bar"]
                            gtk::SearchBar {
                                set_show_close_button: true,
                                set_search_mode: false,

                                #[wrap(Some)]
                                #[name = "table_search"]
                                set_child = &gtk::SearchEntry {
                                    set_placeholder_text: Some(bookie_app_services::tr!("Filter tables…").as_str()),
                                    set_hexpand: true,
                                },
                            },

                            #[name = "sidebar_scroll"]
                            gtk::ScrolledWindow {
                                set_hscrollbar_policy: gtk::PolicyType::Never,
                                set_vexpand: true,
                            },
                        },
                    },

                    #[wrap(Some)]
                    #[name = "toast_overlay"]
                    set_content = &adw::ToastOverlay {
                        #[wrap(Some)]
                        #[name = "content_holder"]
                        set_child = &adw::ToolbarView {
                            #[name = "update_banner"]
                            add_top_bar = &adw::Banner {
                                set_revealed: false,
                                set_use_markup: false,
                                set_button_label: Some(bookie_app_services::tr!("Review").as_str()),
                            },
                            #[name = "reconnect_banner"]
                            add_top_bar = &adw::Banner {
                                set_revealed: false,
                                set_use_markup: false,
                                set_button_label: Some(bookie_app_services::tr!("Retry").as_str()),
                            },
                        },
                    },
                },
            },
        }
    }

    fn init(init: Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let AppInit {
            registry,
            persistence,
            workspace,
            history,
            database,
            preferences,
            mcp_bridge,
        } = init;
        let widgets = view_output!();
        sidebar_layout::install(&widgets.split_view);
        widgets
            .table_search
            .update_property(&[gtk::accessible::Property::Label(&bookie_app_services::tr!(
                "Search tables"
            ))]);
        widgets
            .sidebar_fold_button
            .update_property(&[gtk::accessible::Property::Label(&bookie_app_services::tr!(
                "Collapse all"
            ))]);
        let schema_filter_list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        widgets.schema_filter_popover.set_child(Some(&schema_filter_list));

        init_css::install_pending_change_css();
        let change_trackers = crate::services::change_tracker::WorkspaceChangeTrackers::default();
        let structure_trackers = crate::services::structure_tracker::WorkspaceStructureTrackers::default();
        let window_handles = init_window::install_window_lifecycle(
            &widgets.window,
            &widgets.split_view,
            &widgets.sidebar_toggle,
            &structure_trackers,
            &change_trackers,
            &workspace,
            &sender,
        );
        let sidebar = init_sidebar::build_sidebar(&widgets, &sender);
        let workspace_chrome = init_workspace::build_workspace_chrome(&widgets, &sender);
        let disconnect_action = shortcuts::install_window_actions(&widgets.window, sender.clone());
        shortcuts::install_window_shortcuts(&widgets.window);
        widgets
            .primary_menu_button
            .set_menu_model(Some(&shortcuts::primary_menu_model()));

        let welcome_view = WelcomeView::builder()
            .launch(WelcomeViewInit {
                registry: registry.clone(),
            })
            .forward(sender.input_sender(), |out| match out {
                WelcomeViewOutput::OpenConnect => AppMsg::OpenConnect,
                WelcomeViewOutput::ImportUrl => AppMsg::ImportConnectionUrl,
                WelcomeViewOutput::OpenSaved(saved) => AppMsg::OpenSaved(*saved),
                WelcomeViewOutput::ToggleFavorite(id) => AppMsg::ToggleConnectionFavorite(id),
                WelcomeViewOutput::Organize(saved) => AppMsg::OrganizeConnection(*saved),
                WelcomeViewOutput::Edit(saved) => AppMsg::EditConnection(*saved),
                WelcomeViewOutput::Duplicate(id) => AppMsg::DuplicateConnection(id),
                WelcomeViewOutput::ExportBundle => AppMsg::ExportConnections,
                WelcomeViewOutput::ImportBundle => AppMsg::ImportBundle,
                WelcomeViewOutput::Delete(id) => AppMsg::DeleteConnection(id),
            });

        let current_preferences = preferences.load();
        adw::StyleManager::default().set_color_scheme(crate::services::preferences::color_scheme_style(
            current_preferences.color_scheme,
        ));
        crate::ui::theme::apply(current_preferences.theme);
        crate::ui::typography::apply(&current_preferences);
        crate::ui::editor::apply_font_preferences(&current_preferences);
        crate::ui::grid::apply_typography(&current_preferences.grid_typography);
        crate::ui::grid::set_null_display(current_preferences.null_display);
        crate::ui::grid::set_date_format(current_preferences.date_format);
        crate::ui::grid::full_value_editor::set_show_invisible_characters(
            current_preferences.show_invisible_characters,
        );
        let default_page_size = current_preferences.default_page_size;
        let mut model = App {
            change_trackers,
            structure_trackers,
            registry,
            persistence,
            workspace,
            history,
            database,
            window_state: window_handles.window_state.clone(),
            window_close_state_saved: window_handles.window_close_state_saved.clone(),
            window_close_state_pending: window_handles.window_close_state_pending.clone(),
            preferences,
            mcp_bridge,
            window: root.clone(),
            split_view: widgets.split_view.clone(),
            sidebar_toggle: widgets.sidebar_toggle.clone(),
            sidebar_reveal: std::rc::Rc::default(),
            window_title: widgets.window_title.clone(),
            header_bar: widgets.header_bar.clone(),
            environment_badge: widgets.environment_badge.clone(),
            sidebar_title: widgets.sidebar_title.clone(),
            disconnect_action,
            sidebar_model: sidebar.model,
            sidebar_selection: sidebar.selection,
            sidebar_list: sidebar.list,
            sidebar_row_factories: sidebar.row_factories,
            sidebar_expansion_handlers: sidebar.expansion_handlers,
            sidebar_render_context: sidebar.render_context,
            sidebar_nodes: sidebar.nodes,
            sidebar_collapsed: sidebar.collapsed,
            sidebar_hidden_schemas: sidebar.hidden_schemas,
            schema_filter_button: widgets.schema_filter_button.clone(),
            sidebar_fold_button: widgets.sidebar_fold_button.clone(),
            schema_filter_list,
            sidebar_views: Vec::new(),
            sidebar_tables: Vec::new(),
            sidebar_catalog: Default::default(),
            sidebar_catalog_cancel: Default::default(),
            catalog_generation: std::cell::Cell::new(0),
            catalog_cancel: Default::default(),
            catalog_dialog_cancel: Default::default(),
            database_list_cancel: Default::default(),
            content_holder: widgets.content_holder.clone(),
            history_paned: gtk::Paned::builder()
                .orientation(gtk::Orientation::Vertical)
                .vexpand(true)
                .wide_handle(true)
                .build(),
            toast_overlay: widgets.toast_overlay.clone(),
            connect_progress_toast: None,
            connect_cancel: None,
            reconnect_banner: widgets.reconnect_banner.clone(),
            update_banner: widgets.update_banner.clone(),
            available_update: None,
            connections_factory: workspace_chrome.connections_factory,
            connections_popover: widgets.connections_popover.clone(),
            databases_button: widgets.databases_button.clone(),
            databases_list: database_switcher::build_database_popover(&widgets.databases_popover, &sender),
            pending_database_switch: None,
            connected_database: None,
            workspace_save_token: None,
            health_state: None,
            row_op_spinner: widgets.row_op_spinner.clone(),
            read_only_badge: widgets.read_only_badge.clone(),
            table_search: widgets.table_search.clone(),
            workspace_outer_stack: workspace_chrome.outer_stack,
            workspace_root: None,
            workspace_tab_view: None,
            workspace_tab_bar: None,
            read_scopes: std::cell::RefCell::default(),
            recent_pages: std::cell::RefCell::default(),
            recent_walk: std::cell::RefCell::default(),
            workspace_root_added: std::cell::Cell::new(false),
            overview_front_connection_id: std::rc::Rc::new(std::cell::Cell::new(None)),
            workspace_tabs: std::rc::Rc::new(std::cell::RefCell::new(Default::default())),
            dialog: None,
            favorites: Vec::new(),
            favorites_order: crate::services::request_generation::LatestApplied::default(),
            schema_index: std::rc::Rc::new(std::cell::RefCell::new(crate::ui::editor::SchemaIndex::default())),
            history_dialog: None,
            saved_queries_dialog: None,
            welcome_view,
            current_driver_id: None,
            table_names: Vec::new(),
            read_only: false,
            default_page_size,
            saved_connections: Vec::new(),
            connection_organization: ConnectionOrganizationIndex::default(),
            pending_import: None,
            pending_bundle: None,
            connected: false,
            connection_id: None,
            multi: multi_session::MultiSessionState::default(),
            csv_import_progress: None,
            backup_progress: None,
            switch: ConnectionSwitch {
                transition: ConnectionTransition::Idle,
                prepared: None,
                saves_pending: std::collections::HashMap::new(),
                cancel_audit_was_disabled: None,
            },
            grid_close: grid_close::GridCloseState::new(window_handles.grid_edits_ready),
            close_after_save: window_handles.close_after_save,
            close_window_after_save: window_handles.close_window_after_save,
            force_close: window_handles.force_close,
            in_flight_saves: window_handles.in_flight_saves,
            active_browse_saves: std::collections::HashMap::new(),
            session_teardown_ready: window_handles.session_teardown_ready,
            session_teardown_pending: window_handles.session_teardown_pending,
            structure_saves_in_flight: std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashSet::new())),
            persist_pending: std::rc::Rc::new(std::cell::Cell::new(false)),
            persist_timeout: std::rc::Rc::new(std::cell::RefCell::new(None)),
            persist_generation: std::rc::Rc::new(std::cell::Cell::new(0)),
            closed_tabs_stack: std::rc::Rc::new(std::cell::RefCell::new(std::collections::VecDeque::with_capacity(
                CLOSED_TABS_CAPACITY,
            ))),
            extra_windows: Vec::new(),
            poll_health_source: None,
            history_prune_source: None,
        };
        open_request::forward_open_signal(sender.input_sender());
        sender.input(AppMsg::ReloadConnections);
        sender.input(AppMsg::MaybeShowWhatsNew);
        sender.input(AppMsg::QuietCheckForUpdates);
        model.content_holder.set_content(Some(&model.history_paned));
        model.set_sidebar_connected(false);
        model.show_welcome_page(sender.clone());

        widgets
            .new_connection_button
            .update_property(&[gtk::accessible::Property::Label("New connection")]);
        widgets
            .saved_connections_button
            .update_property(&[gtk::accessible::Property::Label("Open saved connection")]);
        widgets
            .primary_menu_button
            .update_property(&[gtk::accessible::Property::Label("Main menu")]);

        let banner_sender = sender.clone();
        widgets.reconnect_banner.connect_button_clicked(move |_| {
            banner_sender.input(AppMsg::RefreshPage);
        });
        let update_banner_sender = sender.clone();
        widgets.update_banner.connect_button_clicked(move |_| {
            update_banner_sender.input(AppMsg::ReviewAvailableUpdate);
        });

        let poll_sender = sender.clone();
        model.poll_health_source = Some(glib::timeout_add_seconds_local(1, move || {
            poll_sender.input(AppMsg::PollHealth);
            glib::ControlFlow::Continue
        }));

        let history = model.history.clone();
        let preferences_for_prune = model.preferences.clone();
        model.history_prune_source = Some(glib::timeout_add_seconds_local(3600, move || {
            let retention = preferences_for_prune.load().history_retention_days;
            let history = history.clone();
            relm4::spawn(async move {
                if let Some(history) = history
                    && let Err(e) = history.prune_older_than(retention).await
                {
                    tracing::warn!(error = %e, "history prune failed");
                }
            });
            glib::ControlFlow::Continue
        }));

        model.load_favorites(sender.clone());
        model.load_connection_organization(sender.clone());

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        if self.defer_for_grid_edits(&msg, &sender) {
            return;
        }
        match msg {
            AppMsg::FlushWindowEdits => lifecycle_messages::flush_window_edits(self),
            AppMsg::GridEditsPrepared { intent, ticket, result } => {
                self.grid_edits_prepared(intent, ticket, result, sender)
            }
            AppMsg::OpenConnect => self.on_open_connect(sender),
            AppMsg::ConnectionPrepared(prepared) => self.on_connection_prepared(prepared, sender),
            AppMsg::ConnectionPrepareFailed(message, saved) => {
                self.on_connection_prepare_failed(message, *saved, sender)
            }
            AppMsg::ConnectionSwitchDecision(decision) => self.on_connection_switch_decision(decision, sender),
            AppMsg::Disconnect => self.on_disconnect(sender),
            AppMsg::ForceDisconnect => self.request_disconnect(sender),
            AppMsg::PrepareWindowClose => self.prepare_window_close(sender),
            AppMsg::PromptWindowClose => self.prompt_window_close_with_pending(sender),
            AppMsg::PersistWindowStateForClose {
                width,
                height,
                maximized,
            } => lifecycle_messages::persist_window_state_for_close(self, width, height, maximized, sender),
            AppMsg::WindowStatePersisted(result) => lifecycle_messages::window_state_persisted(result),
            AppMsg::WindowStatePersistedForClose(result) => {
                lifecycle_messages::window_state_persisted_for_close(self, result)
            }
            AppMsg::ConnectionDeleted(id) => self.on_connection_deleted(id, sender),
            AppMsg::ConfirmSessionTeardown(action) => self.prepare_editor_sessions(action, sender),
            AppMsg::CancelSessionTeardown(action) => lifecycle_messages::cancel_session_teardown(self, action),
            AppMsg::SessionTeardownCompleted { action, result } => {
                self.on_session_teardown_completed(action, result, sender);
            }
            AppMsg::DialogClosed => self.on_connect_dialog_closed(),
            AppMsg::SelectTable {
                schema,
                name,
                open_mode,
            } => self.on_select_table(schema, name, open_mode, sender),
            AppMsg::OpenReferencedRow { schema, table, filter } => {
                self.on_open_referenced_row(schema, table, filter, sender)
            }
            AppMsg::ChooseReferencedRow(targets) => self.on_choose_referenced_row(targets, sender),
            AppMsg::BrowseViewRestriction(tab_id, restriction) => {
                self.dispatch_to_tab(tab_id, BrowseTabInput::ViewRestriction(restriction));
            }
            AppMsg::ColumnsLoaded(tab_id, columns, request) => {
                self.on_browse_columns_loaded(tab_id, columns, request, sender)
            }
            AppMsg::FetchBrowseForeignKeys(tab_id) => self.fetch_browse_foreign_keys(tab_id, sender),
            AppMsg::FetchBrowseEnumLabels(tab_id, targets) => self.fetch_browse_enum_labels(tab_id, targets, sender),
            AppMsg::EnumLabelsLoaded(tab_id, labels) => {
                self.dispatch_to_tab(tab_id, crate::ui::browse_tab::BrowseTabInput::EnumLabelsLoaded(labels))
            }
            AppMsg::ForeignKeysLoaded(tab_id, foreign_keys) => self.on_browse_foreign_keys_loaded(tab_id, foreign_keys),
            AppMsg::InboundForeignKeysLoaded(tab_id, inbound) => {
                self.on_browse_inbound_foreign_keys_loaded(tab_id, inbound)
            }
            AppMsg::RowsLoaded(tab_id, request, result, projected_columns, elapsed) => {
                self.on_browse_rows_loaded(tab_id, request, result, projected_columns, elapsed)
            }
            AppMsg::LoadFailed(tab_id, msg) => self.on_browse_load_failed(tab_id, msg),
            AppMsg::RowCountLoaded(tab_id, request, count) => self.on_browse_row_count_loaded(tab_id, request, count),
            AppMsg::RowCountFailed(tab_id, request) => self.on_browse_row_count_failed(tab_id, request),
            AppMsg::FetchBrowsePage(tab_id) => self.fetch_browse_page(tab_id, sender),
            AppMsg::FetchBrowseCellValue(tab_id, request, col_index, column_name, row_key) => {
                self.fetch_browse_cell_value(tab_id, request, col_index, column_name, row_key, sender)
            }
            AppMsg::BrowseCellValueLoaded(tab_id, request, col_index, column_name, result) => {
                self.dispatch_to_tab(
                    tab_id,
                    match result {
                        Ok(value) => BrowseTabInput::CellValueLoaded {
                            request,
                            col_index,
                            column_name,
                            value,
                        },
                        Err(message) => BrowseTabInput::CellValueFailed(request, message),
                    },
                );
            }
            AppMsg::FetchBrowseColumns(tab_id) => self.fetch_browse_columns(tab_id, sender),
            AppMsg::FetchBrowseRowCount(tab_id) => self.fetch_browse_row_count(tab_id, sender),
            AppMsg::WorkspaceTabsChanged => self.on_workspace_tabs_changed(sender),
            AppMsg::FocusWorkingSession(id) => self.focus_working_session(id, sender),
            AppMsg::SwitchToRecentTab => self.on_switch_to_recent_tab(),
            AppMsg::EndRecentTabWalk => self.on_end_recent_tab_walk(),
            AppMsg::WorkspaceSchemaWordsChanged => self.refresh_schema_index(),
            AppMsg::ExecuteBrowseTransaction {
                tab_id,
                save_id,
                statements,
                sources,
            } => {
                self.on_execute_browse_transaction(tab_id, save_id, statements, sources, sender);
            }
            AppMsg::SaveCompletedForTab(tab_id, save_id, warning) => {
                self.on_browse_save_completed(tab_id, save_id, warning, sender);
            }
            AppMsg::SaveFailedForTab(tab_id, save_id, message) => {
                self.on_browse_save_failed(tab_id, save_id, message);
            }
            AppMsg::FlashErrorRowForTab(tab_id, source) => {
                self.dispatch_to_tab(tab_id, BrowseTabInput::FlashErrorRow(source.clone()));
                self.dispatch_to_editor(tab_id, SqlEditorInput::ResultFlashError(source));
            }
            AppMsg::FocusActiveGrid => {
                if let Some(id) = self.selected_editor_tab_id() {
                    self.dispatch_to_editor(id, crate::ui::editor::SqlEditorInput::FocusResultGrid);
                } else if let Some(id) = self.selected_browse_tab_id() {
                    self.dispatch_to_tab(id, BrowseTabInput::FocusGrid);
                }
            }
            AppMsg::FocusActiveEditor => {
                if let Some(id) = self.selected_editor_tab_id() {
                    self.dispatch_to_editor(id, crate::ui::editor::SqlEditorInput::FocusEditor);
                }
            }
            AppMsg::SaveActiveBrowseTab => {
                if let Some(id) = self.selected_editor_tab_id() {
                    if self.change_trackers.with_tab_ref(id, |tracker| tracker.has_pending()) == Some(true) {
                        self.dispatch_to_editor(id, SqlEditorInput::CommitResultEdits);
                    } else {
                        self.save_editor_file(id, false, sender);
                    }
                } else if let Some(id) = self.selected_browse_tab_id() {
                    self.dispatch_to_tab(id, BrowseTabInput::CommitSave);
                }
            }
            AppMsg::SaveActiveBrowseTabById(id) => {
                self.dispatch_to_tab(id, BrowseTabInput::CommitSave);
            }
            AppMsg::UndoActiveBrowseTab => {
                // Ctrl+Z routes to BrowseTab undo only. Structure
                // editing follows the snapshot+diff model — DDL undo
                // is a session-level Discard, not a per-keystroke
                // history. Inside a Structure-mode Entry, the native
                // `gtk::Text` undo handles per-field text revert.
                if let Some(id) = self.selected_browse_tab_id() {
                    self.dispatch_to_tab(id, BrowseTabInput::Undo);
                }
            }
            AppMsg::RedoActiveBrowseTab => {
                if let Some(id) = self.selected_browse_tab_id() {
                    self.dispatch_to_tab(id, BrowseTabInput::Redo);
                }
            }
            AppMsg::WorkspaceTabClosed(id) => self.close_workspace_tab_by_id(id, sender),
            AppMsg::FinishCloseWorkspaceTab(id) => {
                if let Some(tab_view) = self.workspace_tab_view.clone() {
                    self.finish_close_workspace_tab(id, &tab_view, sender);
                }
            }
            AppMsg::CloseDiscardedWorkspaceTab(id) => {
                if let Some(tab_view) = self.workspace_tab_view.clone() {
                    self.close_workspace_tab_after_pending(id, &tab_view, sender);
                }
            }
            AppMsg::CloseOtherWorkspaceTabs(id) => self.close_other_workspace_tabs(id, sender),
            AppMsg::CloseWorkspaceTabsToRight(id) => self.close_workspace_tabs_to_right(id, sender),
            AppMsg::CloseActiveWorkspaceTab => self.close_active_workspace_tab(sender),
            AppMsg::ShowAlert { title, body } => self.show_error_alert(&title, &body),
            AppMsg::ExportResults { result, name, scope } => super::export_dialog::present(
                &self.window,
                &self.toast_overlay,
                super::export_dialog::ExportRequest {
                    scope,
                    paged: None,
                    result,
                    suggested_name: name,
                    driver_id: self.driver_id().to_string(),
                },
                &self.preferences,
            ),
            AppMsg::ExportStreamedResults(request) => self.on_export_streamed_results(request),
            AppMsg::ShowToast(msg) => self.show_toast(&msg),
            AppMsg::BrowseTabDirtyChanged(tab_id, dirty) => self.refresh_browse_tab_dirty(tab_id, dirty),
            AppMsg::EditorResultDirtyChanged(tab_id, dirty) => self.refresh_editor_result_dirty(tab_id, dirty),
            AppMsg::SidebarGroupExpansionChanged { key, expanded } => {
                self.sidebar_group_expansion_changed(&key, expanded);
            }
            AppMsg::SidebarSearchChanged => self.refresh_sidebar_model(),
            AppMsg::RefreshSidebar => self.refresh_sidebar(sender),
            AppMsg::RevealInSidebar => self.on_reveal_in_sidebar(),
            AppMsg::HideSidebarSchema(schema) => self.hide_sidebar_schema(&schema, &sender),
            AppMsg::ShowSidebarSchema(schema) => self.show_sidebar_schema(&schema, &sender),
            AppMsg::ShowAllSidebarSchemas => self.show_all_sidebar_schemas(&sender),
            AppMsg::NewTableTab { schema } => self.on_new_table_tab(schema, sender),
            AppMsg::EditStructureTab { schema, table } => self.on_edit_structure_tab(schema, table, sender),
            AppMsg::TableText { schema, table, text } => self.on_structure_text(schema, table, text, sender),
            AppMsg::ShowCreateTableLoaded { sql } => self.append_editor_tab(Some(sql), sender),
            AppMsg::RenameTablePrompt { schema, table } => self.on_rename_table_prompt(schema, table, sender),
            AppMsg::RenameTableConfirmed {
                origin,
                schema,
                table,
                new_name,
            } => self.on_rename_table_confirmed(origin, schema, table, new_name, sender),
            AppMsg::TruncateTablePrompt { schema, table } => self.on_truncate_table_prompt(schema, table, sender),
            AppMsg::TruncateTableConfirmed { origin, schema, table } => {
                self.on_truncate_table_confirmed(origin, schema, table, sender)
            }
            AppMsg::DropTablePrompt { schema, table } => self.on_drop_table_prompt(schema, table, sender),
            AppMsg::DropTablePromptReady {
                origin,
                schema,
                table,
                referencing,
            } => self.present_drop_table_prompt(origin, schema, table, &referencing, sender),
            AppMsg::DropTableConfirmed { origin, schema, table } => {
                self.on_drop_table_confirmed(origin, schema, table, sender)
            }
            AppMsg::DropTableSucceeded { origin, schema, table } => {
                self.on_drop_table_succeeded(origin, schema, table, sender)
            }
            AppMsg::CatalogDdl(message) => self.dispatch_catalog_ddl(message, sender),
            AppMsg::ExecuteStructureTransaction { tab_id, statements } => {
                self.on_execute_structure_transaction(tab_id, statements, sender)
            }
            AppMsg::AttachWorkspaceTab { tab_id, connection_id } => self.attach_tab_to(tab_id, connection_id),
            AppMsg::SaveActiveStructureTabById(id) => self.save_structure_tab_by_id(id, sender),
            AppMsg::StructureSaveCompleted {
                origin,
                tab_id,
                new_table_name,
            } => {
                self.on_structure_save_completed(origin, tab_id, new_table_name, sender.clone());
                self.connection_switch_save_succeeded(tab_id, sender);
            }
            AppMsg::StructureSaveFailed(tab_id, message) => {
                self.on_structure_save_failed(tab_id, message);
                self.connection_switch_save_failed();
            }
            AppMsg::FetchStructureData { tab_id } => self.on_fetch_structure_data(tab_id, sender),
            AppMsg::StructureDataLoaded {
                tab_id,
                columns,
                metadata,
            } => self.on_structure_data_loaded(tab_id, columns, metadata),
            AppMsg::StructureLoadFailed { tab_id, message } => self.on_structure_load_failed(tab_id, message),
            AppMsg::FetchCollations { tab_id } => self.on_fetch_collations(tab_id, sender),
            AppMsg::CollationsLoaded {
                tab_id,
                collations,
                table_default_supported,
            } => self.on_collations_loaded(tab_id, collations, table_default_supported),
            AppMsg::StructureTabDirtyChanged(tab_id, dirty) => self.refresh_structure_tab_dirty(tab_id, dirty),
            AppMsg::SchemaChanged { origin, schema, table } => self.on_schema_changed(origin, schema, table, sender),
            AppMsg::CatalogReloaded {
                origin,
                generation,
                result,
            } => self.on_catalog_reloaded(origin, generation, result, sender.clone()),
            AppMsg::RowOpStarted => self.set_row_op_in_flight(true),
            AppMsg::ReloadConnections => self.on_reload_connections(sender),
            AppMsg::ConnectionsLoaded(connections) => {
                let conns = connections;
                self.on_connections_loaded(&conns, sender);
            }
            AppMsg::NewEditorTab => self.append_editor_tab(None, sender),
            AppMsg::OpenSqlFile => self.on_open_sql_file(sender),
            AppMsg::SqlFileOpened(file) => self.on_sql_file_opened(file, sender),
            AppMsg::SqlFileFailed(message) => self.show_toast(&message),
            AppMsg::SaveActiveEditorFileAs => {
                if let Some(id) = self.selected_editor_tab_id() {
                    self.save_editor_file_as(id, sender);
                }
            }
            AppMsg::SaveEditorFileAs(tab) => self.save_editor_file_as(tab, sender),
            AppMsg::SaveEditorFile { tab, overwrite } => self.save_editor_file(tab, overwrite, sender),
            AppMsg::EditorFileSaved(tab, file) => self.on_editor_file_saved(tab, file),
            AppMsg::EditorFileSaveFailed(tab, message) => self.on_editor_file_save_failed(tab, &message),
            AppMsg::EditorFileSaveCancelled(tab) => self.on_editor_file_save_cancelled(tab),
            AppMsg::EditorFileChangedOnDisk(tab) => self.on_editor_file_changed_on_disk(tab, sender),
            AppMsg::EditorFileRestored(tab, outcome) => self.on_editor_file_restored(tab, outcome),
            AppMsg::EditorTabRunStateChanged(id, running) => {
                self.on_editor_tab_run_state_changed(id, running);
                self.continue_connection_switch(sender);
            }
            AppMsg::EditorTabQueryChanged(id, text) => self.on_editor_tab_query_changed(id, text),
            AppMsg::FavoritesLoaded(ticket, favorites) => self.on_favorites_loaded(ticket, favorites),
            AppMsg::PersistFavorite(favorite) => self.on_persist_favorite(favorite, sender),
            AppMsg::FavoriteSaved => self.show_toast(&bookie_app_services::tr!("Saved as favorite")),
            AppMsg::FavoriteSaveFailed(reason) => self.show_toast(&reason),
            AppMsg::SaveQueryAsFavorite => self.on_save_query_as_favorite(sender),
            AppMsg::ShowSavedQueries => self.on_show_saved_queries(sender),
            AppMsg::SavedQueriesDialogClosed => self.saved_queries_dialog = None,
            AppMsg::SavedQueriesChanged(saved) => {
                let ticket = self.favorites_order.begin();
                self.on_favorites_loaded(ticket, saved);
            }
            AppMsg::ShowQuickSwitcher => self.on_show_quick_switcher(sender),
            AppMsg::QuickSwitcherHistoryLoaded(history) => self.present_quick_switcher(history, sender),
            AppMsg::QuickSwitcherChose(target) => self.on_quick_switcher_chose(target, sender),
            AppMsg::ShowHistory => self.on_show_history(sender),
            AppMsg::CloseHistory => self.hide_history_panel(),
            AppMsg::OpenHistoryQuery(text) => {
                if self.connected {
                    self.append_editor_tab(Some(text), sender);
                } else {
                    self.show_toast(&bookie_app_services::tr!("Connect to a database first to run SQL."));
                }
            }
            AppMsg::ReplaceActiveTabQuery(text) => {
                if self.connected {
                    self.on_replace_active_tab_query(text, sender);
                } else {
                    self.show_toast(&bookie_app_services::tr!("Connect to a database first to run SQL."));
                }
            }
            AppMsg::PollHealth => self.on_poll_health(),
            AppMsg::RefreshPage => self.on_refresh_active_tab(),
            AppMsg::ShowShortcuts => self.on_show_shortcuts(),
            AppMsg::ShowAbout => self.on_show_about(),
            AppMsg::CheckForUpdates => whats_new::start_update_check(self, UpdateCheckSource::Manual, sender.clone()),
            AppMsg::QuietCheckForUpdates => {
                whats_new::start_update_check(self, UpdateCheckSource::Quiet, sender.clone())
            }
            AppMsg::UpdateCheckFinished { source, result } => {
                whats_new::present_update_check_result(self, source, result, sender.clone())
            }
            AppMsg::ReviewAvailableUpdate => whats_new::review_available_update(self, sender.clone()),
            AppMsg::DismissAvailableUpdate => whats_new::dismiss_available_update(self),
            AppMsg::MaybeShowWhatsNew => whats_new::maybe_show_whats_new(self, sender.clone()),
            AppMsg::WhatsNewDismissed => {}
            AppMsg::ShowAuditLog => {
                crate::ui::audit_dialog::present(self.window.upcast_ref::<gtk::Window>(), &self.database)
            }
            AppMsg::ShowAuditOperation(operation_id) => crate::ui::audit_dialog::present_for_operation(
                self.window.upcast_ref::<gtk::Window>(),
                &self.database,
                operation_id,
            ),
            AppMsg::ShowActivity => {
                crate::ui::activity_dialog::present(
                    self.window.upcast_ref::<gtk::Window>(),
                    self.connection_id,
                    &self.database,
                    &self.preferences,
                );
            }
            AppMsg::ShowHealthReport => self.show_health_report(&sender),
            AppMsg::OpenSqlInEditor(sql) => self.append_editor_tab(Some(sql), sender),
            AppMsg::ShowCatalog => self.show_catalog(None),
            AppMsg::ShowCatalogKind(kind) => self.show_catalog(Some(kind)),
            AppMsg::LoadSidebarCatalog(kind) => self.on_load_sidebar_catalog(kind, &sender),
            AppMsg::SidebarCatalogLoaded { origin, kind, result } => {
                self.on_sidebar_catalog_loaded(&origin, kind, result, sender.clone());
            }
            AppMsg::ToggleAllSidebarGroups => self.toggle_all_sidebar_groups(),
            AppMsg::ShowErDiagram { schema } => self.on_show_er_diagram(schema),
            AppMsg::CompareSchema { schema } => self.on_compare_schema(schema, &sender),
            AppMsg::ExplainActiveQuery => self.on_explain_active_query(),
            AppMsg::ShowPreferences => super::preferences::present(
                &self.window,
                self.history.clone(),
                &self.database,
                &self.preferences,
                self.mcp_bridge.clone(),
            ),
            AppMsg::NewWindow => {
                let ctrl = App::builder()
                    .launch(AppInit {
                        registry: self.registry.clone(),
                        persistence: self.persistence.clone(),
                        workspace: self.workspace.clone(),
                        history: self.history.clone(),
                        database: self.database.clone(),
                        preferences: self.preferences.clone(),
                        mcp_bridge: self.mcp_bridge.clone(),
                    })
                    .detach();
                // Only the window relm4 starts the application with is
                // presented for us. A window spawned here stays unmapped
                // unless it joins the application and is presented.
                if let Some(application) = self.window.application() {
                    ctrl.widget().set_application(Some(&application));
                }
                ctrl.widget().present();
                // Dropping a Controller is what actually runs the child's
                // Component::shutdown (which closes its connection and
                // cancels its timers), but nothing does that when the user
                // just closes the window -- the GTK widget is destroyed
                // while this Vec keeps the Controller alive forever. Watch
                // for that destruction and remove our own entry so the
                // Controller drops.
                let key = ctrl.widget().as_ptr() as usize;
                let destroy_sender = sender.clone();
                session_restore::forget_group_on_close(ctrl.widget().upcast_ref(), key, sender.clone());
                ctrl.widget().connect_destroy(move |_| {
                    destroy_sender.input(AppMsg::ExtraWindowClosed(key));
                });
                self.extra_windows.push((key, ctrl));
            }
            AppMsg::ExtraWindowClosed(key) => {
                self.extra_windows.retain(|(k, _)| *k != key);
            }
            AppMsg::ExtraWindowDismissed(key) => self.forget_open_sessions_window(key, sender),
            AppMsg::JumpToColumn => {
                let opened = self
                    .workspace_tab_view
                    .as_ref()
                    .and_then(|view| view.selected_page())
                    .is_some_and(|page| crate::ui::grid::jump_to_column(&page.child()));
                if !opened {
                    self.show_toast(&bookie_app_services::tr!("Open a result grid to jump to a column."));
                }
            }
            AppMsg::ExportCsv => self.on_export(ExportFormat::Csv),
            AppMsg::ExportJson => self.on_export(ExportFormat::Json),
            AppMsg::ExportAllCsv => self.on_export_all(ExportFormat::Csv),
            AppMsg::ExportAllJson => self.on_export_all(ExportFormat::Json),
            AppMsg::ExportFrontResults => self.on_export_front_results(),
            AppMsg::CopyToClipboard(text) => self.on_copy_to_clipboard(text),
            AppMsg::CopyRowAsStatement {
                tab_id,
                row_positions,
                statement,
            } => self.on_copy_row_as_statement(tab_id, &row_positions, statement),
            AppMsg::DeleteConnection(id) => self.on_delete_connection(id, sender),
            AppMsg::CancelConnect => self.on_cancel_connect(),
            AppMsg::ConnectionCancelled => self.on_connection_cancelled(),
            AppMsg::ContinueSessionRestore => self.continue_session_restore(sender),
            AppMsg::EditConnection(saved) => self.on_edit_connection(saved, sender),
            AppMsg::EditConnectionLoaded(prefill) => self.open_connect_dialog(Some(*prefill), sender),
            AppMsg::DuplicateConnection(id) => self.on_duplicate_connection(id, sender),
            AppMsg::ExportConnections => self.on_export_connections(sender),
            AppMsg::ExportConnectionsTo(choice) => self.on_export_connections_to(choice, sender),
            AppMsg::ExportConnectionsSucceeded(count) => self.on_export_connections_succeeded(count),
            AppMsg::ImportBundle => self.on_import_bundle(sender),
            AppMsg::ImportBundleFile(path) => self.on_import_bundle_file(path, sender),
            AppMsg::ImportBundleEncrypted(sealed) => self.on_import_bundle_encrypted(sealed, sender),
            AppMsg::ImportBundleUnlock(passphrase) => self.on_import_bundle_unlock(passphrase, sender),
            AppMsg::ImportBundlePlanned(pending) => self.on_import_bundle_planned(pending, sender),
            AppMsg::ImportBundleConfirmed(choice) => self.on_import_bundle_confirmed(choice, sender),
            AppMsg::ImportBundleFinished(imported, planned) => self.on_import_bundle_finished(imported, planned),
            AppMsg::BundleFailed(message) => self.on_bundle_failed(&message),
            AppMsg::DuplicateConnectionSucceeded {
                copy,
                prompt_for_password,
            } => self.on_duplicate_connection_succeeded(copy, prompt_for_password, sender),
            AppMsg::DuplicateConnectionFailed => self.on_duplicate_connection_failed(),
            AppMsg::ConnectionOrganizationLoaded(index) => self.on_connection_organization_loaded(index),
            AppMsg::ToggleConnectionFavorite(id) => self.on_toggle_connection_favorite(id, sender),
            AppMsg::OrganizeConnection(saved) => self.on_organize_connection(saved, sender),
            AppMsg::SetConnectionOrganization(id, organization) => {
                self.on_set_connection_organization(id, organization, sender)
            }
            AppMsg::ConnectionOrganizationRejected => self.on_connection_organization_rejected(),
            AppMsg::ImportConnectionUrl => self.on_import_connection_url(sender),
            AppMsg::ImportConnectionUrlText(url, connect) => self.on_import_connection_url_text(url, connect, sender),
            AppMsg::OpenLaunchArguments(arguments) => self.on_open_launch_arguments(arguments, sender),
            AppMsg::ImportConnectionUrlSucceeded(name) => self.on_import_connection_url_succeeded(name),
            AppMsg::ImportConnectionUrlFailed => self.on_import_connection_url_failed(),
            AppMsg::OpenSaved(saved) => self.on_open_saved(saved, sender),
            AppMsg::LoadDatabases => self.on_load_databases(sender),
            AppMsg::DatabasesLoaded { origin, result } => self.on_databases_loaded(origin, result, sender),
            AppMsg::SwitchDatabase(name) => self.on_switch_database(name, sender),
            AppMsg::ReopenClosedTab => self.on_reopen_closed_tab(sender),
            AppMsg::ImportCsvIntoTable { schema, table } => self.on_import_csv_into_table(schema, table, sender),
            AppMsg::CreateTableFromCsv { schema } => self.on_create_table_from_csv(schema, sender),
            AppMsg::CsvFileChosen {
                schema,
                table,
                path,
                more_files,
            } => self.on_csv_file_chosen(schema, table, path, more_files, sender),
            AppMsg::CsvImportPrepared(preparation) => self.on_csv_import_prepared(*preparation, sender),
            AppMsg::CsvImportPreparationFailed(failure) => self.on_csv_import_preparation_failed(*failure),
            AppMsg::StartCsvImport(choice) => self.on_start_csv_import(*choice, sender),
            AppMsg::CsvImportPlanPrepared(preparation) => self.on_csv_import_plan_prepared(*preparation, sender),
            AppMsg::CsvImportProgress(committed) => self.on_csv_import_progress(committed),
            AppMsg::CsvImportFinished(report) => self.on_csv_import_finished(*report, sender),
            AppMsg::ImportQueueFileStarted(started) => self.on_import_queue_file_started(*started),
            AppMsg::ImportQueueFinished(finished) => self.on_import_queue_finished(*finished),
            AppMsg::BackUpDatabase => self.on_back_up_database(sender),
            AppMsg::SearchDatabaseValues => self.on_search_database_values(&sender),
            AppMsg::RestoreDatabase => self.on_restore_database(sender),
            AppMsg::StartBackupJob(job) => self.on_start_backup_job(job, sender),
            AppMsg::BackupJobFinished(report) => self.on_backup_job_finished(*report, sender),
            AppMsg::ShowFilterDialog => self.on_show_filter_dialog(),
        }
    }

    /// Runs once for every window, including the primary one on full
    /// application shutdown (relm4 guarantees this). Closing a window's
    /// GTK widget alone does not release what it owns: the DatabaseService
    /// entry (server session, SSH tunnel, reconnect-monitor task) stays
    /// registered under this window's connection id, and the health-poll
    /// and history-prune timers keep firing on the process-wide main
    /// context, until this runs (H12).
    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        self.schema_index.borrow_mut().invalidate_columns();
        self.connection_id = None;
        self.clear_multi_session_state();
        if let Some(source) = self.poll_health_source.take() {
            source.remove();
        }
        if let Some(source) = self.history_prune_source.take() {
            source.remove();
        }
    }
}
