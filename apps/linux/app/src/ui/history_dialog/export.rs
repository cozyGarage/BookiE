use super::*;
use std::path::{Path, PathBuf};

impl HistoryDialog {
    pub(super) fn export_failed(&self, message: &str) {
        tracing::warn!(error = %message, "history export failed");
        let dialog = adw::AlertDialog::builder()
            .heading(crate::tr!("Could not export query history"))
            .body(crate::tr!(
                "The history export was not saved. Choose a writable local destination and try again."
            ))
            .build();
        dialog.add_response("close", &crate::tr!("Close"));
        dialog.set_default_response(Some("close"));
        dialog.set_close_response("close");
        dialog.present(Some(&self.root));
    }

    pub(super) fn save_export(&self, content: String, suggested_name: String, sender: ComponentSender<Self>) {
        let filter = gtk::FileFilter::new();
        if suggested_name.ends_with(".sql") {
            filter.set_name(Some(&crate::tr!("SQL files")));
            filter.add_mime_type("text/x-sql");
            filter.add_suffix("sql");
        } else {
            filter.set_name(Some(&crate::tr!("CSV files")));
            filter.add_mime_type("text/csv");
            filter.add_suffix("csv");
        }
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title(crate::tr!("Export query history"))
            .modal(true)
            .initial_name(&suggested_name)
            .default_filter(&filter)
            .filters(&filters)
            .build();
        let parent_window = self.root.root().and_then(|r| r.downcast::<gtk::Window>().ok());
        dialog.save(
            parent_window.as_ref(),
            gio::Cancellable::NONE,
            move |outcome| match export_destination(outcome) {
                Ok(Some(path)) => {
                    sender.spawn_oneshot_command(move || HistoryDialogCmd::ExportWritten(write_export(&path, &content)))
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = sender.command_sender().send(HistoryDialogCmd::ExportFailed(error));
                }
            },
        );
    }
}

fn export_destination(outcome: Result<gio::File, glib::Error>) -> Result<Option<PathBuf>, String> {
    match outcome {
        Ok(file) => file
            .path()
            .map(Some)
            .ok_or_else(|| crate::tr!("Choose a local file for history export.")),
        Err(error) if error.matches(gtk::DialogError::Dismissed) || error.matches(gtk::DialogError::Cancelled) => {
            Ok(None)
        }
        Err(error) => Err(error.to_string()),
    }
}

fn write_export(path: &Path, content: &str) -> Result<(), String> {
    tablepro_core::export::write_atomically(path, |output| output.write_all(content.as_bytes()))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_export_keeps_exact_sql_and_csv_bytes() {
        let directory = tempfile::tempdir().unwrap();
        for (name, content) in [
            ("history.sql", "SELECT 'é';\n"),
            ("history.csv", "query\r\n\"SELECT 'é'\"\r\n"),
        ] {
            let path = directory.path().join(name);
            std::fs::write(&path, "previous export").unwrap();
            assert_eq!(write_export(&path, content), Ok(()));
            assert_eq!(std::fs::read(&path).unwrap(), content.as_bytes());
        }
    }

    #[test]
    fn a_failed_export_preserves_the_destination_and_removes_temporary_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("existing");
        std::fs::create_dir(&path).unwrap();
        let existing = path.join("previous.sql");
        std::fs::write(&existing, "SELECT 'keep'").unwrap();
        assert!(write_export(&path, "replacement").is_err());
        assert_eq!(std::fs::read_to_string(existing).unwrap(), "SELECT 'keep'");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn cancelling_the_file_chooser_does_not_report_a_write_failure() {
        for kind in [gtk::DialogError::Cancelled, gtk::DialogError::Dismissed] {
            assert_eq!(export_destination(Err(glib::Error::new(kind, "cancelled"))), Ok(None));
        }
        assert!(export_destination(Err(glib::Error::new(gtk::DialogError::Failed, "failed"))).is_err());
    }

    #[test]
    fn only_local_export_destinations_are_accepted() {
        let local = PathBuf::from("/synthetic/history.sql");
        assert_eq!(export_destination(Ok(gio::File::for_path(&local))), Ok(Some(local)));
        assert!(export_destination(Ok(gio::File::for_uri("sftp://invalid/history.sql"))).is_err());
    }
}
