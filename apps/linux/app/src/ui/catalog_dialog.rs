use std::cell::RefCell;
use std::rc::Rc;

use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use tokio_util::sync::CancellationToken;

use bookie_core::{CATALOG_OBJECT_LIMIT, CatalogObject, CatalogObjectKind, DriverError};

use crate::services::database_service::DatabaseService;
use crate::services::preferences::PreferencesStore;
use crate::services::request_generation::replace_scoped_in_flight;
use bookie_app_services::tr;

struct CatalogSource {
    connection: uuid::Uuid,
    database: std::sync::Arc<DatabaseService>,
    preferences: PreferencesStore,
    connection_scope: CancellationToken,
    hidden_schemas: Vec<String>,
    can_restart_sequences: bool,
    in_flight: Rc<RefCell<CancellationToken>>,
}

pub struct CatalogRequest {
    pub initial_kind: Option<CatalogObjectKind>,
    pub hidden_schemas: Vec<String>,
    pub can_restart_sequences: bool,
}

pub fn present(
    parent: &gtk::Window,
    connection_id: Option<uuid::Uuid>,
    database: &std::sync::Arc<DatabaseService>,
    preferences: &PreferencesStore,
    connection_scope: CancellationToken,
    request: CatalogRequest,
) {
    let CatalogRequest {
        initial_kind,
        hidden_schemas,
        can_restart_sequences,
    } = request;
    let Some(connection) = connection_id.filter(|id| database.metadata(*id).is_some()) else {
        let alert = adw::AlertDialog::new(
            Some(&tr!("No active connection")),
            Some(&tr!("Open a connection before browsing its catalog.")),
        );
        alert.add_response("ok", &tr!("OK"));
        alert.present(Some(parent));
        return;
    };
    let source = Rc::new(CatalogSource {
        connection,
        database: database.clone(),
        preferences: preferences.clone(),
        connection_scope: connection_scope.clone(),
        hidden_schemas,
        can_restart_sequences,
        in_flight: Rc::new(RefCell::new(connection_scope.child_token())),
    });
    let dialog = adw::Dialog::builder().title(tr!("Catalog")).build();
    dialog.set_content_width(720);
    dialog.set_content_height(560);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    let kinds = kind_dropdown(initial_kind);
    let schema = gtk::SearchEntry::builder()
        .placeholder_text(tr!("All schemas"))
        .hexpand(true)
        .build();
    let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).build();
    list.add_css_class("boxed-list");
    let status = gtk::Label::builder().xalign(0.0).build();
    status.add_css_class("dim-label");
    toolbar.set_content(Some(&catalog_layout(&kinds, &schema, &list, &status)));
    dialog.set_child(Some(&toolbar));

    let reload = reload_action(&source, &kinds, &schema, &list, &status);
    let on_kind = reload.clone();
    kinds.connect_selected_notify(move |_| on_kind());
    let on_schema = reload.clone();
    schema.connect_activate(move |_| on_schema());
    reload();
    let in_flight = source.in_flight.clone();
    dialog.connect_closed(move |_| in_flight.borrow().cancel());
    crate::ui::leak_probe::track("catalog", &dialog);
    dialog.present(Some(parent));
}

fn reload_action(
    source: &Rc<CatalogSource>,
    kinds: &gtk::DropDown,
    schema: &gtk::SearchEntry,
    list: &gtk::ListBox,
    status: &gtk::Label,
) -> Rc<dyn Fn()> {
    let (source, kinds, schema, list, status) = (
        source.clone(),
        kinds.clone(),
        schema.clone(),
        list.clone(),
        status.clone(),
    );
    Rc::new(move || {
        let kind = CatalogObjectKind::ALL
            .get(kinds.selected() as usize)
            .copied()
            .unwrap_or(CatalogObjectKind::Routine);
        let filter = Some(schema.text().trim().to_string()).filter(|text| !text.is_empty());
        load(&source, kind, filter, &list, &status);
    })
}

fn kind_dropdown(initial: Option<CatalogObjectKind>) -> gtk::DropDown {
    let labels: Vec<String> = CatalogObjectKind::ALL.iter().map(|kind| kind_label(*kind)).collect();
    let kinds = steady_width_dropdown(&labels);
    let position = initial.and_then(|kind| CatalogObjectKind::ALL.iter().position(|candidate| *candidate == kind));
    if let Some(index) = position {
        kinds.set_selected(index as u32);
    }
    kinds
}

fn steady_width_dropdown(labels: &[String]) -> gtk::DropDown {
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dropdown = gtk::DropDown::from_strings(&label_refs);
    let labels = labels.to_vec();
    dropdown.connect_realize(move |dropdown| {
        let text_width = |text: &str| dropdown.create_pango_layout(Some(text)).pixel_size().0;
        let selected = labels
            .get(dropdown.selected() as usize)
            .map_or(0, |label| text_width(label));
        let widest = labels.iter().map(|label| text_width(label)).max().unwrap_or(0);
        let natural = dropdown.measure(gtk::Orientation::Horizontal, -1).1;
        dropdown.set_width_request(natural - selected + widest);
    });
    dropdown
}

fn catalog_layout(
    kinds: &gtk::DropDown,
    schema: &gtk::SearchEntry,
    list: &gtk::ListBox,
    status: &gtk::Label,
) -> gtk::Box {
    let filters = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .build();
    filters.append(kinds);
    filters.append(schema);
    let scrolled = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(list)
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    content.append(&filters);
    content.append(&scrolled);
    content.append(status);
    content
}

fn load(
    source: &CatalogSource,
    kind: CatalogObjectKind,
    schema: Option<String>,
    list: &gtk::ListBox,
    status: &gtk::Label,
) {
    list.remove_all();
    let token = replace_scoped_in_flight(&source.in_flight, &source.connection_scope);
    let Some(conn) = source.database.get(source.connection) else {
        status.set_text(&tr!("Connection closed."));
        return;
    };
    status.set_text(&tr!("Loading…"));
    let timeout_secs =
        crate::services::operation_control::timeout_for(&source.preferences, &source.database, Some(source.connection));
    let hidden_schemas = source.hidden_schemas.clone();
    let driver_id = source
        .database
        .metadata(source.connection)
        .map(|metadata| metadata.driver_id);
    let can_restart_sequences = source.can_restart_sequences;
    let (list, status) = (list.clone(), status.clone());
    glib::spawn_future_local(async move {
        let control = crate::services::operation_control::bounded_with(timeout_secs, token.clone());
        let outcome = crate::services::catalog::list_objects_excluding(
            conn.as_ref(),
            kind,
            schema.as_deref(),
            &hidden_schemas,
            &control,
        )
        .await;
        if token.is_cancelled() {
            return;
        }
        if let Ok(objects) = &outcome {
            for object in objects {
                let row = object_row(object);
                if let (CatalogObjectKind::Sequence, true, Some(driver_id)) =
                    (object.kind, can_restart_sequences, driver_id.as_deref())
                {
                    let target = SequenceTarget {
                        connection: conn.clone(),
                        driver_id: driver_id.to_owned(),
                        schema: object.schema.clone(),
                        name: object.name.clone(),
                        timeout_secs,
                    };
                    row.add_suffix(&set_value_button(target, &status));
                }
                list.append(&row);
            }
        }
        status.set_text(&outcome_text(kind, &outcome));
    });
}

struct SequenceTarget {
    connection: std::sync::Arc<dyn bookie_core::Connection>,
    driver_id: String,
    schema: Option<String>,
    name: String,
    timeout_secs: u32,
}

fn set_value_button(target: SequenceTarget, status: &gtk::Label) -> gtk::Button {
    let button = gtk::Button::builder()
        .label(tr!("Set value"))
        .valign(gtk::Align::Center)
        .build();
    let target = Rc::new(target);
    let status = status.clone();
    button.connect_clicked(move |button| prompt_set_value(button, target.clone(), status.clone()));
    button
}

fn prompt_set_value(anchor: &gtk::Button, target: Rc<SequenceTarget>, status: gtk::Label) {
    let dialog = adw::AlertDialog::new(
        Some(&tr!("Set value of {name}").replace("{name}", &target.name)),
        Some(&tr!("The next value handed out will be the number you enter.")),
    );
    let entry = gtk::Entry::builder()
        .placeholder_text(tr!("Next value"))
        .activates_default(true)
        .build();
    dialog.set_extra_child(Some(&entry));
    dialog.add_response("cancel", &tr!("Cancel"));
    dialog.add_response("set", &tr!("Set value"));
    dialog.set_response_appearance("set", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("set"));
    dialog.set_close_response("cancel");
    dialog.connect_response(None, move |_, response| {
        if response != "set" {
            return;
        }
        let text = entry.text().to_string();
        let (target, status) = (target.clone(), status.clone());
        status.set_text(&tr!("Setting value…"));
        glib::spawn_future_local(async move {
            let control = crate::services::operation_control::bounded(target.timeout_secs);
            let outcome = bookie_app_services::sequence_value::set_sequence_value(
                target.connection.as_ref(),
                &target.driver_id,
                target.schema.as_deref(),
                &target.name,
                &text,
                &control,
            )
            .await;
            status.set_text(&set_value_outcome_text(&target.name, &outcome));
        });
    });
    dialog.present(Some(anchor));
}

fn set_value_outcome_text(
    name: &str,
    outcome: &Result<(), bookie_app_services::sequence_value::SetSequenceError>,
) -> String {
    use bookie_app_services::sequence_value::SetSequenceError;
    match outcome {
        Ok(()) => tr!("{name} restarted. Reload the list to see its state.").replace("{name}", name),
        Err(SetSequenceError::Driver(DriverError::PolicyDenied(_))) => {
            tr!("Policy does not allow changing this sequence.")
        }
        Err(SetSequenceError::Driver(error)) => crate::services::error_text::driver_message(error),
        Err(error) => error.to_string(),
    }
}

fn object_row(object: &CatalogObject) -> adw::ActionRow {
    let subtitle = [object.schema.as_deref(), object.detail.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    adw::ActionRow::builder()
        .title(glib::markup_escape_text(&object.name))
        .subtitle(glib::markup_escape_text(&subtitle))
        .build()
}

pub(crate) fn kind_label(kind: CatalogObjectKind) -> String {
    match kind {
        CatalogObjectKind::Routine => tr!("Functions and procedures"),
        CatalogObjectKind::Trigger => tr!("Triggers"),
        CatalogObjectKind::Sequence => tr!("Sequences"),
        CatalogObjectKind::Extension => tr!("Extensions"),
        CatalogObjectKind::Role => tr!("Roles"),
        CatalogObjectKind::Type => tr!("Types"),
        CatalogObjectKind::Grant => tr!("Privileges"),
    }
}

fn outcome_text(kind: CatalogObjectKind, outcome: &Result<Vec<CatalogObject>, DriverError>) -> String {
    match outcome {
        Ok(objects) if objects.is_empty() => tr!("Nothing of this kind was found."),
        Ok(objects) if objects.len() >= CATALOG_OBJECT_LIMIT => {
            tr!("Showing the first {limit}. Narrow the schema to see the rest.")
                .replace("{limit}", &CATALOG_OBJECT_LIMIT.to_string())
        }
        Ok(objects) => tr!("{count} found").replace("{count}", &objects.len().to_string()),
        Err(DriverError::Unsupported(_)) => {
            tr!("This engine does not list {kind}.").replace("{kind}", &kind_label(kind).to_lowercase())
        }
        Err(DriverError::PolicyDenied(_)) => tr!("Policy does not allow this listing."),
        Err(error) => crate::services::error_text::driver_message(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(name: &str) -> CatalogObject {
        CatalogObject {
            kind: CatalogObjectKind::Trigger,
            schema: Some("public".into()),
            name: name.into(),
            detail: None,
        }
    }

    #[test]
    fn empty_unsupported_denied_and_failed_listings_read_differently() {
        let kind = CatalogObjectKind::Trigger;
        let texts = [
            outcome_text(kind, &Ok(Vec::new())),
            outcome_text(kind, &Err(DriverError::Unsupported("x".into()))),
            outcome_text(kind, &Err(DriverError::PolicyDenied("x".into()))),
            outcome_text(kind, &Err(DriverError::Disconnected)),
        ];
        for (index, text) in texts.iter().enumerate() {
            assert!(!text.is_empty());
            assert!(texts.iter().skip(index + 1).all(|other| other != text), "{texts:?}");
        }
        assert!(texts[1].contains("triggers"), "{}", texts[1]);
    }

    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn choosing_a_longer_kind_does_not_change_the_dropdown_width() {
        gtk::init().unwrap();
        let labels: Vec<String> = CatalogObjectKind::ALL.iter().map(|kind| kind_label(*kind)).collect();
        let dropdown = steady_width_dropdown(&labels);
        let window = gtk::Window::builder().child(&dropdown).build();
        window.present();
        let natural_width = |index: usize| {
            dropdown.set_selected(index as u32);
            while gtk::glib::MainContext::default().iteration(false) {}
            dropdown.measure(gtk::Orientation::Horizontal, -1).1
        };
        let widths: Vec<i32> = (0..labels.len()).map(natural_width).collect();
        assert!(widths.iter().all(|width| *width == widths[0]), "{labels:?} {widths:?}");
        window.close();
    }

    #[test]
    fn a_listing_that_reaches_the_limit_says_it_was_cut_short() {
        let full: Vec<CatalogObject> = (0..CATALOG_OBJECT_LIMIT).map(|i| object(&i.to_string())).collect();

        assert!(outcome_text(CatalogObjectKind::Trigger, &Ok(full)).contains("first"));
        assert_eq!(
            outcome_text(CatalogObjectKind::Trigger, &Ok(vec![object("a")])),
            "1 found"
        );
    }

    #[test]
    fn a_policy_denial_and_a_bad_number_get_their_own_messages() {
        use bookie_app_services::sequence_value::SetSequenceError;
        let denied = Err(SetSequenceError::Driver(DriverError::PolicyDenied("read-only".into())));
        let invalid = Err(SetSequenceError::NotANumber);
        let done = Ok(());

        assert_eq!(
            set_value_outcome_text("job_ids", &denied),
            "Policy does not allow changing this sequence."
        );
        assert_eq!(set_value_outcome_text("job_ids", &invalid), "enter a whole number");
        assert!(set_value_outcome_text("job_ids", &done).starts_with("job_ids restarted"));
    }
}
