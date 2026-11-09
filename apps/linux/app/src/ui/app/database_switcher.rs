use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw, gtk};
use uuid::Uuid;

use super::{App, AppMsg};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DatabaseChoice {
    pub name: String,
    pub current: bool,
}

pub(super) fn database_choices(names: &[String], current: &str) -> Vec<DatabaseChoice> {
    let mut choices: Vec<DatabaseChoice> = names
        .iter()
        .map(|name| DatabaseChoice {
            name: name.clone(),
            current: name == current,
        })
        .collect();
    if !current.is_empty() && !choices.iter().any(|choice| choice.current) {
        choices.push(DatabaseChoice {
            name: current.to_string(),
            current: true,
        });
    }
    choices.sort_by_key(|choice| choice.name.to_lowercase());
    choices.dedup_by(|left, right| left.name == right.name);
    choices
}

fn stage_database_switch(
    pending: &mut Option<(Uuid, String)>,
    transition: super::ConnectionTransition,
    connection_id: Uuid,
    database: String,
) -> bool {
    if transition != super::ConnectionTransition::Idle {
        return false;
    }
    *pending = Some((connection_id, database));
    true
}

pub(super) fn build_database_popover(popover: &gtk::Popover, sender: &ComponentSender<App>) -> gtk::ListBox {
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .child(&list)
        .min_content_width(280)
        .min_content_height(80)
        .max_content_height(360)
        .propagate_natural_height(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();
    content.append(&scroll);
    let create = gtk::Button::builder()
        .label(crate::tr!("New Database\u{2026}"))
        .css_classes(["flat"])
        .halign(gtk::Align::Start)
        .build();
    create.set_widget_name("new-database");
    let create_sender = sender.clone();
    create.connect_clicked(move |_| {
        create_sender.input(AppMsg::CatalogDdl(super::msg::CatalogDdlMsg::NewDatabasePrompt))
    });
    content.append(&create);
    popover.set_child(Some(&content));
    let opened = sender.clone();
    popover.connect_visible_notify(move |popover| {
        if popover.is_visible() {
            opened.input(AppMsg::LoadDatabases);
        }
    });
    list
}

fn clear(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn choice_row(choice: &DatabaseChoice, current: &str, driver_id: &str, sender: &ComponentSender<App>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let label = gtk::Label::builder()
        .label(&choice.name)
        .xalign(0.0)
        .hexpand(true)
        .build();
    content.append(&label);
    if choice.current {
        content.append(&gtk::Image::from_icon_name("object-select-symbolic"));
    }
    let button = gtk::Button::builder()
        .child(&content)
        .css_classes(["flat"])
        .hexpand(true)
        .build();
    button.update_property(&[gtk::accessible::Property::Label(&choice.name)]);
    let picked = sender.clone();
    let name = choice.name.clone();
    button.connect_clicked(move |_| picked.input(AppMsg::SwitchDatabase(name.clone())));
    row.append(&button);
    if super::catalog_ddl::droppable_database(&choice.name, current, driver_id) {
        let drop = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text(crate::tr!("Drop {database}\u{2026}").replace("{database}", &choice.name))
            .css_classes(["flat"])
            .build();
        let drop_sender = sender.clone();
        let drop_name = choice.name.clone();
        drop.connect_clicked(move |_| {
            drop_sender.input(AppMsg::CatalogDdl(super::msg::CatalogDdlMsg::DropDatabasePrompt {
                database: drop_name.clone(),
            }));
        });
        row.append(&drop);
    }
    row
}

fn message_row(title: &str, subtitle: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .use_markup(false)
        .title(title)
        .subtitle(subtitle)
        .build()
}

fn find_named_button(widget: &gtk::Widget, name: &str) -> Option<gtk::Button> {
    if let Some(button) = widget.downcast_ref::<gtk::Button>()
        && button.widget_name() == name
    {
        return Some(button.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(button) = find_named_button(&current, name) {
            return Some(button);
        }
        child = current.next_sibling();
    }
    None
}

impl App {
    pub(super) fn on_load_databases(&self, sender: ComponentSender<Self>) {
        let Some(origin) = crate::services::catalog::CatalogOrigin::capture(self.connection_id, &self.database) else {
            return;
        };
        let Some(connection) = origin.connection(&self.database) else {
            return;
        };
        clear(&self.databases_list);
        self.databases_list.append(&message_row(&crate::tr!("Loading…"), ""));
        let timeout_secs =
            crate::services::operation_control::timeout_for(&self.preferences, &self.database, self.connection_id);
        let cancel = crate::services::request_generation::replace_in_flight(&self.database_list_cancel);
        let reply = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let control = crate::services::operation_control::bounded_with(timeout_secs, cancel.clone());
                    let result = connection
                        .list_databases_controlled(&control)
                        .await
                        .map_err(|error| crate::services::error_text::driver_message(&error));
                    if cancel.is_cancelled() {
                        return;
                    }
                    reply.input(AppMsg::DatabasesLoaded { origin, result });
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_databases_loaded(
        &self,
        origin: crate::services::catalog::CatalogOrigin,
        result: Result<Vec<String>, String>,
        sender: ComponentSender<Self>,
    ) {
        if !origin.owns_window(self.connection_id, &self.database) {
            return;
        }
        let connection_id = origin.id;
        clear(&self.databases_list);
        let names = match result {
            Ok(names) => names,
            Err(message) => {
                self.databases_list
                    .append(&message_row(&crate::tr!("Could not list databases"), &message));
                return;
            }
        };
        let current = self
            .saved_connections
            .iter()
            .find(|saved| saved.id == connection_id)
            .map(|saved| saved.database.clone())
            .unwrap_or_default();
        let driver_id = self.driver_id();
        if let Some(create) = self
            .databases_button
            .popover()
            .and_then(|popover| popover.child())
            .and_then(|child| find_named_button(&child, "new-database"))
        {
            create.set_visible(bookie_core::sql_ddl::supports_create_database(driver_id));
        }
        for choice in database_choices(&names, &current) {
            self.databases_list
                .append(&choice_row(&choice, &current, driver_id, &sender));
        }
    }

    pub(super) fn on_switch_database(&mut self, name: String, sender: ComponentSender<Self>) {
        self.databases_button.popdown();
        let Some(id) = self.connection_id else {
            return;
        };
        let Some(mut saved) = self.saved_connections.iter().find(|saved| saved.id == id).cloned() else {
            self.show_toast(&crate::tr!(
                "This connection is not saved, so its database cannot be switched."
            ));
            return;
        };
        if saved.database == name {
            return;
        }
        saved.database = name.clone();
        if !stage_database_switch(&mut self.pending_database_switch, self.switch.transition, id, name) {
            self.on_open_saved(saved, sender);
            return;
        }
        self.on_open_saved(saved, sender);
    }

    pub(super) fn show_database_switcher(&mut self, driver_id: &str, id: Uuid, sender: &ComponentSender<Self>) {
        self.databases_button.set_visible(
            self.registry
                .get(driver_id)
                .is_some_and(|driver| driver.capabilities().supports_database_listing),
        );
        if let Some(create) = self
            .databases_button
            .popover()
            .and_then(|popover| popover.child())
            .and_then(|child| find_named_button(&child, "new-database"))
        {
            create.set_visible(bookie_core::sql_ddl::supports_create_database(driver_id));
        }
        self.persist_switched_database(id, sender);
    }

    fn persist_switched_database(&mut self, id: Uuid, sender: &ComponentSender<Self>) {
        let Some((pending, name)) = self.pending_database_switch.take() else {
            return;
        };
        if pending != id {
            return;
        }
        let reply = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let update = bookie_storage::update_connection(id, |connection| connection.database = name).await;
                    if let Err(error) = update {
                        tracing::warn!(%error, "the switched database was not saved");
                    }
                    reply.input(AppMsg::ReloadConnections);
                })
                .drop_on_shutdown()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn databases_are_listed_alphabetically_with_the_current_one_marked() {
        let choices = database_choices(&names(&["zeta", "Alpha", "beta"]), "beta");
        assert_eq!(
            choices
                .iter()
                .map(|choice| (choice.name.as_str(), choice.current))
                .collect::<Vec<_>>(),
            vec![("Alpha", false), ("beta", true), ("zeta", false)]
        );
    }

    #[test]
    fn a_current_database_the_server_did_not_list_is_still_shown() {
        let choices = database_choices(&names(&["alpha"]), "hidden_db");
        assert!(
            choices
                .iter()
                .any(|choice| choice.name == "hidden_db" && choice.current)
        );
        assert_eq!(choices.len(), 2);
    }

    #[test]
    fn an_empty_current_name_adds_nothing_and_duplicates_collapse() {
        let choices = database_choices(&names(&["a", "a"]), "");
        assert_eq!(choices.len(), 1);
        assert!(!choices[0].current);
    }

    #[test]
    fn a_busy_connection_transition_does_not_create_a_pending_database_switch() {
        let active_connection_id = Uuid::new_v4();
        let requested_connection_id = Uuid::new_v4();
        for transition in [
            super::super::ConnectionTransition::Connecting,
            super::super::ConnectionTransition::AwaitingDecision,
            super::super::ConnectionTransition::WaitingForRuns,
            super::super::ConnectionTransition::WaitingForSaves,
        ] {
            let existing = Some((active_connection_id, "active".to_owned()));
            let mut pending = existing.clone();
            assert!(!stage_database_switch(
                &mut pending,
                transition,
                requested_connection_id,
                "next".to_owned()
            ));
            assert_eq!(pending, existing);
        }
        let mut pending = None;
        assert!(stage_database_switch(
            &mut pending,
            super::super::ConnectionTransition::Idle,
            requested_connection_id,
            "next".to_owned()
        ));
        assert_eq!(pending, Some((requested_connection_id, "next".to_owned())));
    }
}
