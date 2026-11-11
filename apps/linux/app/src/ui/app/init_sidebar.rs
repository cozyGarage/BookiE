use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use relm4::adw::prelude::*;
use relm4::factory::FactoryVecDeque;
use relm4::prelude::*;
use relm4::{adw, gtk};

use super::structure::StructureText;
use super::{App, AppMsg, AppWidgets, OpenMode};
use crate::services::sidebar_tree::{CollapseState, NodeKind, TreeNode};
use crate::ui::sidebar_row::{SidebarRow, SidebarRowOutput};
use crate::ui::sidebar_tree_model::SidebarTreeModel;

pub(super) type RowFactories = Rc<RefCell<HashMap<gtk::ListItem, FactoryVecDeque<SidebarRow>>>>;
pub(super) type ExpansionHandlers = Rc<RefCell<HashMap<gtk::ListItem, (gtk::TreeListRow, gtk::glib::SignalHandlerId)>>>;

#[derive(Default)]
pub(super) struct SidebarRenderContext {
    pub(super) driver_id: String,
    pub(super) schema_filter_enabled: bool,
    pub(super) column_source: Option<crate::ui::sidebar_columns::ColumnSource>,
}

pub(super) struct SidebarParts {
    pub(super) model: SidebarTreeModel,
    pub(super) selection: gtk::SingleSelection,
    pub(super) list: gtk::ListView,
    pub(super) row_factories: RowFactories,
    pub(super) expansion_handlers: ExpansionHandlers,
    pub(super) render_context: Rc<RefCell<SidebarRenderContext>>,
    pub(super) nodes: Rc<RefCell<Vec<TreeNode>>>,
    pub(super) collapsed: Rc<RefCell<CollapseState>>,
    pub(super) hidden_schemas: Rc<RefCell<HashSet<String>>>,
}

pub(super) fn build_sidebar(widgets: &AppWidgets, sender: &ComponentSender<App>) -> SidebarParts {
    let nodes = Rc::new(RefCell::new(Vec::<TreeNode>::new()));
    let collapsed = Rc::new(RefCell::new(CollapseState::default()));
    let hidden_schemas = Rc::new(RefCell::new(HashSet::new()));
    let render_context = Rc::new(RefCell::new(SidebarRenderContext::default()));
    let model = SidebarTreeModel::new(&[]);
    model.tree().set_autoexpand(true);
    let selection = gtk::SingleSelection::new(Some(model.tree().clone()));
    selection.set_autoselect(false);
    selection.set_can_unselect(true);
    let row_factories = Rc::new(RefCell::new(HashMap::new()));
    let expansion_handlers = Rc::new(RefCell::new(HashMap::new()));
    let factory = build_list_item_factory(
        sender,
        &row_factories,
        &expansion_handlers,
        &nodes,
        &collapsed,
        &render_context,
        &widgets.table_search,
    );
    let list = gtk::ListView::new(Some(selection.clone()), Some(factory));
    list.add_css_class("navigation-sidebar");
    list.set_single_click_activate(true);
    connect_activation(&list, &model, &nodes, sender);
    let placeholder = empty_sidebar_placeholder();
    let overlay = gtk::Overlay::builder().child(&list).build();
    overlay.add_overlay(&placeholder);
    placeholder.set_halign(gtk::Align::Center);
    placeholder.set_valign(gtk::Align::Center);
    placeholder.set_visible(true);
    widgets.sidebar_scroll.set_child(Some(&overlay));
    let placeholder_for_items = placeholder.clone();
    let roots = model.roots().clone();
    roots.connect_items_changed(move |roots, _, _, _| placeholder_for_items.set_visible(roots.n_items() == 0));
    connect_search(widgets, sender);
    SidebarParts {
        model,
        selection,
        list,
        row_factories,
        expansion_handlers,
        render_context,
        nodes,
        collapsed,
        hidden_schemas,
    }
}

fn build_list_item_factory(
    sender: &ComponentSender<App>,
    row_factories: &RowFactories,
    expansion_handlers: &ExpansionHandlers,
    nodes: &Rc<RefCell<Vec<TreeNode>>>,
    collapsed: &Rc<RefCell<CollapseState>>,
    render_context: &Rc<RefCell<SidebarRenderContext>>,
    search: &gtk::SearchEntry,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    connect_setup(&factory, sender, row_factories);
    connect_bind(
        &factory,
        row_factories,
        &expansion_handlers,
        nodes,
        collapsed,
        render_context,
        search,
        sender,
    );
    connect_unbind(&factory, row_factories, &expansion_handlers);
    factory
}

fn connect_setup(factory: &gtk::SignalListItemFactory, sender: &ComponentSender<App>, row_factories: &RowFactories) {
    let sender = sender.clone();
    let row_factories = row_factories.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let tree_expander = gtk::TreeExpander::new();
        tree_expander.set_hide_expander(true);
        tree_expander.set_indent_for_depth(false);
        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .activate_on_single_click(false)
            .css_classes(["navigation-sidebar"])
            .build();
        tree_expander.set_child(Some(&list_box));
        item.set_child(Some(&tree_expander));
        let row_factory = FactoryVecDeque::<SidebarRow>::builder()
            .launch(list_box)
            .forward(sender.input_sender(), map_sidebar_output);
        row_factories.borrow_mut().insert(item.clone(), row_factory);
    });
}

fn connect_bind(
    factory: &gtk::SignalListItemFactory,
    row_factories: &RowFactories,
    expansion_handlers: &ExpansionHandlers,
    nodes: &Rc<RefCell<Vec<TreeNode>>>,
    collapsed: &Rc<RefCell<CollapseState>>,
    render_context: &Rc<RefCell<SidebarRenderContext>>,
    search: &gtk::SearchEntry,
    sender: &ComponentSender<App>,
) {
    let row_factories = row_factories.clone();
    let expansion_handlers = expansion_handlers.clone();
    let nodes = nodes.clone();
    let collapsed = collapsed.clone();
    let render_context = render_context.clone();
    let search = search.clone();
    let sender = sender.clone();
    factory.connect_bind(move |_, object| {
        bind_list_item(
            object,
            &row_factories,
            &expansion_handlers,
            &nodes,
            &collapsed,
            &render_context,
            &search,
            &sender,
        );
    });
}

fn bind_list_item(
    object: &gtk::glib::Object,
    row_factories: &RowFactories,
    expansion_handlers: &ExpansionHandlers,
    nodes: &Rc<RefCell<Vec<TreeNode>>>,
    collapsed: &Rc<RefCell<CollapseState>>,
    render_context: &Rc<RefCell<SidebarRenderContext>>,
    search: &gtk::SearchEntry,
    sender: &ComponentSender<App>,
) {
    let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
        return;
    };
    let (Some(tree_row), Some(expander)) = (
        item.item().and_downcast::<gtk::TreeListRow>(),
        item.child().and_downcast::<gtk::TreeExpander>(),
    ) else {
        return;
    };
    let Some(index) = SidebarTreeModel::node_index(&tree_row) else {
        return;
    };
    let Some(node) = nodes.borrow().get(index).cloned() else {
        return;
    };
    expander.set_list_row(Some(&tree_row));
    let context = render_context.borrow();
    let init = super::sidebar_state::row_for(
        &node,
        &collapsed.borrow(),
        !search.text().trim().is_empty(),
        context.schema_filter_enabled,
        &context.driver_id,
        context.column_source.clone(),
    );
    if let Some(factory) = row_factories.borrow_mut().get_mut(item) {
        let mut guard = factory.guard();
        guard.clear();
        guard.push_back(init);
    }
    bind_expansion(
        item,
        &tree_row,
        &node,
        collapsed,
        search,
        sender,
        row_factories,
        expansion_handlers,
    );
}

fn bind_expansion(
    item: &gtk::ListItem,
    row: &gtk::TreeListRow,
    node: &TreeNode,
    collapsed: &Rc<RefCell<CollapseState>>,
    search: &gtk::SearchEntry,
    sender: &ComponentSender<App>,
    row_factories: &RowFactories,
    expansion_handlers: &ExpansionHandlers,
) {
    if !is_expandable(node) {
        return;
    }
    if let Some((previous_row, handler)) = expansion_handlers.borrow_mut().remove(item) {
        previous_row.disconnect(handler);
    }
    let key = node.key.clone();
    let kind = node.kind.clone();
    let searching = !search.text().trim().is_empty();
    let collapsed_state = collapsed.clone();
    let sender = sender.clone();
    let row_factories = row_factories.clone();
    let item_for_callback = item.clone();
    let handler = row.connect_expanded_notify(move |row| {
        if let Some(factory) = row_factories.borrow().get(&item_for_callback) {
            factory.send(
                0,
                crate::ui::sidebar_row::SidebarRowMsg::SetCollapsed(!row.is_expanded()),
            );
        }
        if searching {
            if !row.is_expanded() {
                row.set_expanded(true);
            }
            return;
        }
        if let NodeKind::Catalog(kind) = &kind {
            if row.is_expanded() {
                sender.input(AppMsg::LoadSidebarCatalog(*kind));
            }
            return;
        }
        if collapsed_state.borrow().is_collapsed(&key) == row.is_expanded() {
            sender.input(AppMsg::SidebarGroupExpansionChanged {
                key: key.clone(),
                expanded: row.is_expanded(),
            });
        }
    });
    expansion_handlers
        .borrow_mut()
        .insert(item.clone(), (row.clone(), handler));
    let searching = !search.text().trim().is_empty();
    let expanded = match &node.kind {
        NodeKind::Catalog(_) => false,
        _ if searching => true,
        _ => !collapsed.borrow().is_collapsed(&node.key),
    };
    if row.is_expanded() != expanded {
        set_expansion_after_bind(item, row, expanded);
    }
}

fn set_expansion_after_bind(item: &gtk::ListItem, row: &gtk::TreeListRow, expanded: bool) {
    let item = item.clone();
    let row = row.clone();
    gtk::glib::idle_add_local_once(move || {
        let still_bound = item
            .item()
            .and_downcast::<gtk::TreeListRow>()
            .is_some_and(|bound| bound == row);
        if still_bound && row.is_expanded() != expanded {
            row.set_expanded(expanded);
        }
    });
}

fn connect_unbind(
    factory: &gtk::SignalListItemFactory,
    row_factories: &RowFactories,
    expansion_handlers: &ExpansionHandlers,
) {
    let row_factories = row_factories.clone();
    let expansion_handlers = expansion_handlers.clone();
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        if let Some(row_factory) = row_factories.borrow_mut().get_mut(item) {
            row_factory.guard().clear();
        }
        if let Some(expander) = item.child().and_downcast::<gtk::TreeExpander>() {
            expander.set_list_row(None::<&gtk::TreeListRow>);
        }
        if let Some((row, handler)) = expansion_handlers.borrow_mut().remove(item) {
            row.disconnect(handler);
        }
    });
}

fn connect_activation(
    list: &gtk::ListView,
    model: &SidebarTreeModel,
    nodes: &Rc<RefCell<Vec<TreeNode>>>,
    sender: &ComponentSender<App>,
) {
    let tree = model.tree().clone();
    let nodes = nodes.clone();
    let sender = sender.clone();
    list.connect_activate(move |_, position| {
        let Some(row) = tree.row(position) else {
            return;
        };
        let Some(index) = SidebarTreeModel::node_index(&row) else {
            return;
        };
        let node = nodes.borrow().get(index).cloned();
        let Some(node) = node else {
            return;
        };
        activate_node(&node, &row, &sender);
    });
}

fn activate_node(node: &TreeNode, row: &gtk::TreeListRow, sender: &ComponentSender<App>) {
    match &node.kind {
        NodeKind::Table { schema, name } | NodeKind::View { schema, name } => sender.input(AppMsg::SelectTable {
            schema: schema.clone(),
            name: name.clone(),
            open_mode: OpenMode::SwitchOrAppend,
        }),
        NodeKind::Schema | NodeKind::Group(_) | NodeKind::CatalogLoaded(_) => row.set_expanded(!row.is_expanded()),
        NodeKind::Catalog(_) => row.set_expanded(!row.is_expanded()),
        NodeKind::CatalogObject { kind, .. } | NodeKind::CatalogMore(kind) => {
            sender.input(AppMsg::ShowCatalogKind(*kind));
        }
    }
}

pub(super) fn is_expandable(node: &TreeNode) -> bool {
    matches!(
        &node.kind,
        NodeKind::Schema | NodeKind::Group(_) | NodeKind::Catalog(_) | NodeKind::CatalogLoaded(_)
    )
}

pub(super) fn disconnect_expansion_handlers(handlers: &ExpansionHandlers) {
    for (_, (row, handler)) in handlers.borrow_mut().drain() {
        row.disconnect(handler);
    }
}

fn empty_sidebar_placeholder() -> adw::StatusPage {
    let page = adw::StatusPage::builder()
        .icon_name("view-list-symbolic")
        .title(bookie_app_services::tr!("No tables"))
        .description(bookie_app_services::tr!(
            "Nothing matches the current search, or this connection has no tables yet. Use Refresh tables to check again."
        ))
        .build();
    page.add_css_class("compact");
    page
}

fn connect_search(widgets: &AppWidgets, sender: &ComponentSender<App>) {
    let sender = sender.clone();
    widgets
        .table_search
        .connect_search_changed(move |_| sender.input(AppMsg::SidebarSearchChanged));
    widgets.table_search_bar.connect_entry(&widgets.table_search);
    widgets
        .table_search_bar
        .set_key_capture_widget(Some(&widgets.sidebar_root));
    widgets
        .table_search_toggle
        .bind_property("active", &widgets.table_search_bar, "search-mode-enabled")
        .bidirectional()
        .sync_create()
        .build();
}

fn map_sidebar_output(output: SidebarRowOutput) -> AppMsg {
    match output {
        SidebarRowOutput::Open { schema, name } => AppMsg::SelectTable {
            schema,
            name,
            open_mode: OpenMode::SwitchOrAppend,
        },
        SidebarRowOutput::OpenInNewTab { schema, name } => AppMsg::SelectTable {
            schema,
            name,
            open_mode: OpenMode::NewTab,
        },
        SidebarRowOutput::EditStructure { schema, name } => AppMsg::EditStructureTab { schema, table: name },
        SidebarRowOutput::ShowCreateTable { schema, name } => AppMsg::TableText {
            schema,
            table: name,
            text: StructureText::CreateTable,
        },
        SidebarRowOutput::JoinReferenced { schema, name } => AppMsg::TableText {
            schema,
            table: name,
            text: StructureText::JoinSkeleton,
        },
        SidebarRowOutput::ImportCsv { schema, name } => AppMsg::ImportCsvIntoTable { schema, table: name },
        SidebarRowOutput::RenameTable { schema, name } => AppMsg::RenameTablePrompt { schema, table: name },
        SidebarRowOutput::TruncateTable { schema, name } => AppMsg::TruncateTablePrompt { schema, table: name },
        SidebarRowOutput::DropTable { schema, name } => AppMsg::DropTablePrompt { schema, table: name },
        SidebarRowOutput::NewTable { schema } => AppMsg::NewTableTab { schema },
        SidebarRowOutput::TableFromCsv { schema } => AppMsg::CreateTableFromCsv { schema },
        SidebarRowOutput::ShowDiagram { schema } => AppMsg::ShowErDiagram { schema },
        SidebarRowOutput::CompareSchema { schema } => AppMsg::CompareSchema { schema },
        SidebarRowOutput::HideSchema(schema) => AppMsg::HideSidebarSchema(schema),
        SidebarRowOutput::NewSchema => AppMsg::CatalogDdl(crate::ui::app::msg::CatalogDdlMsg::NewSchemaPrompt),
        SidebarRowOutput::DropSchema { schema } => {
            AppMsg::CatalogDdl(crate::ui::app::msg::CatalogDdlMsg::DropSchemaPrompt { schema })
        }
    }
}
