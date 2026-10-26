use relm4::adw::prelude::*;
use tablepro_core::{ConnectionFormDefaults, DatabaseDriver, connection_form_defaults};

use crate::services::connection_form::ConnectFormState;

pub(super) fn apply_driver_values(
    dialog: &mut super::ConnectDialog,
    index: u32,
) -> Option<std::sync::Arc<dyn DatabaseDriver>> {
    let previous_index = dialog.applied_driver;
    dialog.applied_driver = Some(index);
    let entry = dialog.drivers.get(index as usize)?;
    let driver = dialog.registry.get(&entry.id)?;
    let defaults = connection_form_defaults(driver.as_ref());
    let previous = previous_index
        .and_then(|previous| dialog.drivers.get(previous as usize))
        .and_then(|entry| dialog.registry.get(&entry.id))
        .map(|driver| connection_form_defaults(driver.as_ref()));
    apply_values(&dialog.port, &dialog.database, &dialog.username, previous, defaults);
    Some(driver)
}

impl super::ConnectDialog {
    pub(super) fn apply_driver_form_visibility(&mut self, driver: &dyn tablepro_core::DatabaseDriver) {
        let capabilities = driver.capabilities();
        self.form = ConnectFormState::from_capabilities(capabilities, self.form.selected);
        if !self.form.endpoint(false).shows_endpoint_choice() {
            self.endpoint_combo.set_selected(0);
        }
        self.apply_form_state();
        self.configure_socket_field(self.form.local_socket_path_kind());
        self.apply_tls_visibility();
        self.database_file_picker.set_visible(self.form.file_based);
        self.auth_source.set_visible(driver.id() == "mongodb");
        self.database.set_title(&if self.form.file_based {
            crate::tr!("File path")
        } else {
            crate::tr!("Database")
        });
    }
}

fn apply_values(
    port: &relm4::gtk::SpinButton,
    database: &relm4::adw::EntryRow,
    username: &relm4::adw::EntryRow,
    previous: Option<ConnectionFormDefaults>,
    next: ConnectionFormDefaults,
) {
    port.set_value(next_driver_value(port.value() as u16, previous.map(|value| value.port), next.port) as f64);
    database.set_text(&next_driver_text(
        &database.text(),
        previous.map(|value| value.database),
        next.database,
    ));
    username.set_text(&next_driver_text(
        &username.text(),
        previous.map(|value| value.username),
        next.username,
    ));
}

pub(super) fn test_result_text(result: &Result<usize, String>) -> String {
    match result {
        Ok(tables) => crate::tr!("Connection ok · {n} table(s) visible").replace("{n}", &tables.to_string()),
        Err(error) => crate::tr!("Test failed: {error}").replace("{error}", error),
    }
}

pub(super) fn default_driver_row(drivers: &[super::DriverEntry]) -> u32 {
    drivers.iter().position(|d| d.id == "postgres").unwrap_or(0) as u32
}

pub(super) fn next_driver_value<T: PartialEq + Copy>(current: T, previous: Option<T>, next: T) -> T {
    if previous.is_some_and(|previous| current != previous) {
        current
    } else {
        next
    }
}

pub(super) fn next_driver_text(current: &str, previous: Option<&str>, next: &str) -> String {
    if previous.is_some_and(|previous| current != previous) {
        current.to_owned()
    } else {
        next.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_drivers_updates_only_fields_still_at_the_previous_default() {
        let port = next_driver_value(5432, Some(5432), 3306);
        let custom_port = next_driver_value(55432, Some(5432), 3306);
        let database = next_driver_text("postgres", Some("postgres"), "mysql");
        let custom_database = next_driver_text("shop", Some("postgres"), "mysql");
        let username = next_driver_text("postgres", Some("postgres"), "root");
        let custom_username = next_driver_text("bookie", Some("postgres"), "root");

        assert_eq!((port, database.as_str(), username.as_str()), (3306, "mysql", "root"));
        assert_eq!(
            (custom_port, custom_database.as_str(), custom_username.as_str()),
            (55432, "shop", "bookie")
        );
    }

    #[test]
    fn the_first_driver_selection_applies_its_defaults() {
        assert_eq!(next_driver_value(0, None, 3306), 3306);
        assert_eq!(next_driver_text("", None, "mysql"), "mysql");
    }

    fn driver(id: &str) -> crate::ui::connect_dialog::DriverEntry {
        crate::ui::connect_dialog::DriverEntry {
            id: id.to_string(),
            display_name: id.to_string(),
            maturity: tablepro_core::DriverMaturity::Stable,
        }
    }

    #[test]
    fn a_new_connection_starts_on_postgres_when_it_is_registered() {
        let drivers = [driver("clickhouse"), driver("mysql"), driver("postgres")];
        assert_eq!(default_driver_row(&drivers), 2);
    }

    #[test]
    fn the_default_driver_falls_back_to_the_first_row() {
        assert_eq!(default_driver_row(&[driver("clickhouse"), driver("sqlite")]), 0);
        assert_eq!(default_driver_row(&[]), 0);
    }

    #[test]
    fn a_test_result_names_the_table_count_or_the_failure() {
        assert_eq!(test_result_text(&Ok(3)), "Connection ok · 3 table(s) visible");
        assert_eq!(
            test_result_text(&Err("password rejected".into())),
            "Test failed: password rejected"
        );
    }
}

pub(super) fn typed_database(text: &str) -> String {
    text.to_string()
}

#[cfg(test)]
mod typed_database_tests {
    use super::typed_database;

    #[test]
    fn database_identifiers_and_file_paths_keep_their_exact_spelling() {
        for value in ["shop", " shop", "shop ", "  shop \n", "", " /data/my file.db "] {
            assert_eq!(typed_database(value), value);
        }
    }
}
