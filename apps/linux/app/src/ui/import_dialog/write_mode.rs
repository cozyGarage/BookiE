use bookie_core::import::{ConflictKey, WriteMode};
use bookie_core::{DriverCapabilities, ImportUpsertStyle};
use relm4::adw::prelude::*;
use relm4::{adw, gtk};

pub(super) struct WriteModeSection {
    group: adw::PreferencesGroup,
    mode: adw::ComboRow,
    key: adw::ComboRow,
    keys: Vec<ConflictKey>,
}

impl WriteModeSection {
    pub(super) fn new(capabilities: &DriverCapabilities, keys: &[ConflictKey]) -> Option<Self> {
        if capabilities.import_upsert == ImportUpsertStyle::Unsupported {
            return None;
        }
        let group = adw::PreferencesGroup::builder()
            .title(bookie_app_services::tr!("Existing rows"))
            .description(bookie_app_services::tr!(
                "Choose what happens when a row in the file has the same key as a row already in the table."
            ))
            .build();
        let mode = adw::ComboRow::builder()
            .use_markup(false)
            .title(bookie_app_services::tr!("When a key already exists"))
            .model(&gtk::StringList::new(&[
                &bookie_app_services::tr!("Insert only (a duplicate key stops the import)"),
                &bookie_app_services::tr!("Update the existing row"),
            ]))
            .build();
        let labels: Vec<String> = keys.iter().map(key_label).collect();
        let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let key = adw::ComboRow::builder()
            .use_markup(false)
            .title(bookie_app_services::tr!("Match rows by"))
            .model(&gtk::StringList::new(&label_refs))
            .sensitive(false)
            .build();
        if keys.is_empty() {
            mode.set_sensitive(false);
            mode.set_subtitle(&bookie_app_services::tr!(
                "This table has no primary key or unique index, so rows cannot be matched."
            ));
        }
        group.add(&mode);
        group.add(&key);
        let section = Self {
            group,
            mode,
            key,
            keys: keys.to_vec(),
        };
        section.follow_mode();
        Some(section)
    }

    fn follow_mode(&self) {
        let key = self.key.downgrade();
        self.mode.connect_selected_notify(move |mode| {
            if let Some(key) = key.upgrade() {
                key.set_sensitive(mode.selected() == 1);
            }
        });
    }

    pub(super) fn group(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    pub(super) fn selection(&self) -> Option<WriteMode> {
        if self.mode.selected() != 1 {
            return Some(WriteMode::Insert);
        }
        let key = self.keys.get(self.key.selected() as usize)?;
        Some(WriteMode::UpdateOnConflict(key.columns.clone()))
    }
}

fn key_label(key: &ConflictKey) -> String {
    format!("{} ({})", key.label, key.columns.join(", "))
}

pub(super) fn queue_notice(more_files: usize) -> Option<adw::PreferencesGroup> {
    if more_files == 0 {
        return None;
    }
    let group = adw::PreferencesGroup::builder()
        .title(bookie_app_services::tr!("More files"))
        .description(
            bookie_app_services::tr!(
                "These choices also apply to {count} more files. Each of them must have the same columns in the same order. The import stops at the first file that fails."
            )
            .replace("{count}", &more_files.to_string()),
        )
        .build();
    Some(group)
}

#[cfg(test)]
#[path = "write_mode_tests.rs"]
mod tests;
