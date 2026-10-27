use relm4::adw::prelude::*;
use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender};
use relm4::{adw, gtk};
use uuid::Uuid;

use tablepro_core::{AuthMode, DriverRegistry, Environment};
use tablepro_storage::{ConnectionOrganization, SavedConnection};

/// What a row needs to render: the saved record plus its organisation
/// entry. The two live in different files on disk, so the parent hands
/// them down together rather than the row reaching for either.
#[derive(Debug, Clone)]
pub struct ConnectionRowInit {
    pub saved: SavedConnection,
    pub organization: ConnectionOrganization,
    pub driver_name: String,
}

impl ConnectionRowInit {
    pub fn new(saved: SavedConnection, organization: ConnectionOrganization, registry: &DriverRegistry) -> Self {
        let driver_name = registry
            .get(&saved.driver_id)
            .map(|driver| driver.display_name().to_string())
            .unwrap_or_else(|| saved.driver_id.clone());
        Self {
            saved,
            organization,
            driver_name,
        }
    }
}

#[derive(Debug)]
pub struct ConnectionRow {
    saved: SavedConnection,
    organization: ConnectionOrganization,
    driver_name: String,
    actions_button: gtk::MenuButton,
    favorite_button: gtk::Button,
    open_button: gtk::Button,
}

#[derive(Debug, Clone)]
pub enum ConnectionRowMsg {
    Open,
    ToggleFavorite,
    Organize,
    Edit,
    Duplicate,
    /// Trash button pressed. Triggers a confirmation dialog before
    /// any actual delete is dispatched — saved connections include
    /// credentials and SSH config and a misclick is unrecoverable.
    RequestDelete,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ConnectionRowOutput {
    Open(SavedConnection),
    ToggleFavorite(Uuid),
    Organize(SavedConnection),
    Edit(SavedConnection),
    Duplicate(Uuid),
    Delete(Uuid),
}

#[relm4::factory(pub)]
impl FactoryComponent for ConnectionRow {
    type Init = ConnectionRowInit;
    type Input = ConnectionRowMsg;
    type Output = ConnectionRowOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        adw::ActionRow {
            set_use_markup: false,
            set_title: &self.saved.name,
            set_title_lines: 1,
            set_subtitle_lines: 1,
            set_tooltip_text: Some(format!("{}\n{}", self.saved.name, subtitle_for(&self.saved, &self.organization, &self.driver_name)).as_str()),
            set_subtitle: &subtitle_for(&self.saved, &self.organization, &self.driver_name),
            set_activatable: true,
            connect_activated => ConnectionRowMsg::Open,

            add_prefix = &gtk::Box {
                set_valign: gtk::Align::Center,
                set_hexpand: false,
                set_width_request: 10,
                set_visible: color_css_class(&self.organization).is_some(),
                set_css_classes: &color_classes(&self.organization),
                set_tooltip_text: self.organization.color.as_deref(),
            },

            add_prefix = &gtk::Box {
                add_css_class: "tp-env-swatch",
                add_css_class: environment_css_class(self.saved.environment),
                set_valign: gtk::Align::Fill,
                set_hexpand: false,
                set_width_request: 6,
                set_tooltip_text: Some(self.saved.environment.display_name()),
            },

            add_prefix = &self.favorite_button.clone(),

            add_suffix = &self.actions_button.clone(),

            add_suffix = &self.open_button.clone(),
        }
    }

    fn init_model(init: Self::Init, _index: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        let favorite = init.organization.favorite;
        Self {
            saved: init.saved,
            organization: init.organization,
            driver_name: init.driver_name,
            actions_button: connection_actions(sender.clone()),
            favorite_button: favorite_button(favorite, sender.clone()),
            open_button: open_button(sender),
        }
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let widgets = view_output!();
        widgets
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            ConnectionRowMsg::Open => {
                let _ = sender.output(ConnectionRowOutput::Open(self.saved.clone()));
            }
            ConnectionRowMsg::ToggleFavorite => {
                let _ = sender.output(ConnectionRowOutput::ToggleFavorite(self.saved.id));
            }
            ConnectionRowMsg::Organize => {
                let _ = sender.output(ConnectionRowOutput::Organize(self.saved.clone()));
            }
            ConnectionRowMsg::Edit => {
                let _ = sender.output(ConnectionRowOutput::Edit(self.saved.clone()));
            }
            ConnectionRowMsg::Duplicate => {
                let _ = sender.output(ConnectionRowOutput::Duplicate(self.saved.id));
            }
            ConnectionRowMsg::RequestDelete => {
                let _ = sender.output(ConnectionRowOutput::Delete(self.saved.id));
            }
        }
    }
}

fn favorite_button(favorite: bool, sender: FactorySender<ConnectionRow>) -> gtk::Button {
    let label = if favorite {
        crate::tr!("Remove from favourites")
    } else {
        crate::tr!("Add to favourites")
    };
    let button = gtk::Button::builder()
        .icon_name(if favorite {
            "starred-symbolic"
        } else {
            "non-starred-symbolic"
        })
        .valign(gtk::Align::Center)
        .tooltip_text(&label)
        .css_classes(["flat"])
        .build();
    button.update_property(&[gtk::accessible::Property::Label(&label)]);
    button.connect_clicked(move |_| sender.input(ConnectionRowMsg::ToggleFavorite));
    button
}

fn open_button(sender: FactorySender<ConnectionRow>) -> gtk::Button {
    let label = crate::tr!("Open connection");
    let button = gtk::Button::builder()
        .icon_name("go-next-symbolic")
        .valign(gtk::Align::Center)
        .tooltip_text(&label)
        .css_classes(["flat"])
        .build();
    button.update_property(&[gtk::accessible::Property::Label(&label)]);
    button.connect_clicked(move |_| sender.input(ConnectionRowMsg::Open));
    button
}

fn connection_actions(sender: FactorySender<ConnectionRow>) -> gtk::MenuButton {
    let button = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .valign(gtk::Align::Center)
        .tooltip_text(crate::tr!("Connection actions"))
        .css_classes(["flat"])
        .build();
    button.update_property(&[gtk::accessible::Property::Label(&crate::tr!("Connection actions"))]);
    let popover = gtk::Popover::new();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
    for (label, message) in [
        (crate::tr!("Group and tags"), ConnectionRowMsg::Organize),
        (crate::tr!("Edit connection"), ConnectionRowMsg::Edit),
        (crate::tr!("Duplicate connection"), ConnectionRowMsg::Duplicate),
        (crate::tr!("Remove connection"), ConnectionRowMsg::RequestDelete),
    ] {
        let action = gtk::Button::with_label(&label);
        action.add_css_class("flat");
        let sender = sender.clone();
        let popover = popover.downgrade();
        action.connect_clicked(move |_| {
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
            sender.input(message.clone());
        });
        content.append(&action);
    }
    popover.set_child(Some(&content));
    button.set_popover(Some(&popover));
    button
}

/// The palette class for a row's colour tag, or `None` when it has no
/// colour or one outside the palette.
fn color_css_class(organization: &ConnectionOrganization) -> Option<&'static str> {
    organization
        .color
        .as_deref()
        .and_then(tablepro_storage::connection_color_css_class)
}

fn color_classes(organization: &ConnectionOrganization) -> Vec<&'static str> {
    let mut classes = vec!["tp-color-swatch"];
    classes.extend(color_css_class(organization));
    classes
}

pub(crate) fn environment_css_class(environment: Environment) -> &'static str {
    match environment {
        Environment::Local => "tp-env-local",
        Environment::Dev => "tp-env-dev",
        Environment::Staging => "tp-env-staging",
        Environment::Prod => "tp-env-prod",
    }
}

fn subtitle_for(saved: &SavedConnection, organization: &ConnectionOrganization, driver_name: &str) -> String {
    let mut subtitle = format!(
        "{} · {} · {}",
        saved.environment.display_name(),
        driver_name,
        endpoint_for(saved)
    );
    if let Some(group) = organization.group.as_deref() {
        subtitle.push_str(" · ");
        subtitle.push_str(group);
    }
    if !organization.tags.is_empty() {
        subtitle.push_str(" · ");
        subtitle.push_str(&organization.tags.join(", "));
    }
    subtitle
}

fn endpoint_for(saved: &SavedConnection) -> String {
    if saved.driver_id == "sqlite" {
        return saved.database.clone();
    }
    if let Some(directory) = &saved.socket_dir {
        return format!("{}@{}/.s.PGSQL.{}", saved.username, directory.display(), saved.port);
    }
    if let Some(path) = &saved.socket_path {
        return format!("{}@{}", saved.username, path.display());
    }
    match saved.auth_mode {
        AuthMode::Kerberos => format!("{}:{}", saved.host, saved.port),
        AuthMode::Password => format!("{}@{}:{}", saved.username, saved.host, saved.port),
    }
}

#[cfg(test)]
#[path = "connection_row_tests.rs"]
mod tests;
