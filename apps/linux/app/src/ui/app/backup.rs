use std::path::PathBuf;

use relm4::ComponentSender;
use relm4::adw::prelude::*;
use relm4::gtk;
use relm4::gtk::gio;
use tablepro_app_services::database_backup::{BackupJob, SavedBackupContext, run_saved_backup_job};
use tablepro_core::{DriverError, OperationControl};
use tablepro_policy::Principal;

use crate::services::error_text;
use crate::ui::progress_dialog::CancellableDialog;

use super::{App, AppMsg};

#[derive(Debug)]
pub struct BackupReport {
    pub job: BackupJob,
    pub error: Option<DriverError>,
}

impl App {
    pub(super) fn on_back_up_database(&self, sender: ComponentSender<Self>) {
        if let Some(message) = self.backup_refusal(false) {
            self.show_error_alert(&crate::tr!("Can't back up"), &message);
            return;
        }
        let filter = archive_filter();
        let dialog = gtk::FileDialog::builder()
            .title(crate::tr!("Back Up Database"))
            .modal(true)
            .initial_name(format!("{}.dump", self.active_database_name()))
            .default_filter(&filter)
            .build();
        dialog.save(Some(&self.window), gio::Cancellable::NONE, move |outcome| {
            let Some(output) = outcome.ok().and_then(|file| file.path()) else {
                return;
            };
            sender.input(AppMsg::StartBackupJob(BackupJob::Backup { output }));
        });
    }

    pub(super) fn on_restore_database(&self, sender: ComponentSender<Self>) {
        if let Some(message) = self.backup_refusal(true) {
            self.show_error_alert(&crate::tr!("Can't restore"), &message);
            return;
        }
        let filter = archive_filter();
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title(crate::tr!("Restore from Backup"))
            .modal(true)
            .default_filter(&filter)
            .filters(&filters)
            .build();
        dialog.open(Some(&self.window), gio::Cancellable::NONE, move |outcome| {
            let Some(input) = outcome.ok().and_then(|file| file.path()) else {
                return;
            };
            sender.input(AppMsg::StartBackupJob(BackupJob::Restore { input }));
        });
    }

    fn backup_refusal(&self, restore: bool) -> Option<String> {
        if self.connection_id.is_none() || self.current_driver_id.is_none() {
            return Some(crate::tr!("No active connection."));
        }
        if self.driver_id() != "postgres" {
            return Some(crate::tr!("Backups are available for PostgreSQL connections."));
        }
        if restore && self.read_only {
            return Some(crate::tr!("This connection is open read-only."));
        }
        if self.backup_progress.is_some() {
            return Some(crate::tr!("A backup or restore is already running."));
        }
        None
    }

    fn active_database_name(&self) -> String {
        self.connection_id
            .and_then(|id| self.saved_connections.iter().find(|saved| saved.id == id))
            .map_or_else(|| "database".to_owned(), |saved| saved.database.clone())
    }

    pub(super) fn on_start_backup_job(&mut self, job: BackupJob, sender: ComponentSender<Self>) {
        let restore = matches!(job, BackupJob::Restore { .. });
        if let Some(message) = self.backup_refusal(restore) {
            self.show_error_alert(&crate::tr!("Can't start"), &message);
            return;
        }
        let Some(id) = self.connection_id else {
            return;
        };
        let saved = self.saved_connections.iter().find(|saved| saved.id == id).cloned();
        let guard = self.database.guard(id, Principal::human_gui());
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        let (Some(saved), Some(guard), Some(runtime_dir)) = (saved, guard, runtime_dir) else {
            self.show_error_alert(
                &crate::tr!("Can't start"),
                &crate::tr!("The connection or the private runtime directory is not available."),
            );
            return;
        };
        let Some(driver) = self.registry.get(&saved.driver_id) else {
            self.show_error_alert(
                &crate::tr!("Can't start"),
                &crate::tr!("The connection or the private runtime directory is not available."),
            );
            return;
        };
        let environment = self
            .database
            .ssh_environment_for_connection(&saved, Principal::human_gui());
        let server_version = self.database.metadata(id).and_then(|metadata| metadata.server_version);
        let title = if restore {
            crate::tr!("Restoring {database}")
        } else {
            crate::tr!("Backing up {database}")
        }
        .replace("{database}", &saved.database);
        let spinner = gtk::Spinner::builder().spinning(true).build();
        let progress = CancellableDialog::present(&self.window, &title, &[spinner.upcast_ref()]);
        let control = OperationControl::new(progress.cancel_token(), None);
        self.backup_progress = Some(progress);
        let finished = sender.clone();
        sender.command(move |_, shutdown| {
            shutdown
                .register(async move {
                    let context = SavedBackupContext {
                        guard: &guard,
                        driver: driver.as_ref(),
                        saved: &saved,
                        environment: &environment,
                        server_version: server_version.as_deref(),
                        runtime_dir: &runtime_dir,
                    };
                    let error = run_saved_backup_job(context, job.clone(), &control).await.err();
                    finished.input(AppMsg::BackupJobFinished(Box::new(BackupReport { job, error })));
                })
                .drop_on_shutdown()
        });
    }

    pub(super) fn on_backup_job_finished(&mut self, report: BackupReport, sender: ComponentSender<Self>) {
        if let Some(progress) = self.backup_progress.take() {
            progress.close();
        }
        let restore = matches!(report.job, BackupJob::Restore { .. });
        match report.error {
            None if restore => {
                self.show_toast(&crate::tr!("The database was restored from the backup."));
                sender.input(AppMsg::RefreshPage);
            }
            None => self.show_toast(&crate::tr!("The backup was saved.")),
            Some(DriverError::Cancelled) => self.show_toast(&crate::tr!("Cancelled. Nothing was changed.")),
            Some(error) => {
                let title = if restore {
                    crate::tr!("Restore failed")
                } else {
                    crate::tr!("Backup failed")
                };
                self.show_error_alert(&title, &backup_error_text(&error));
            }
        }
    }
}

fn backup_error_text(error: &DriverError) -> String {
    let text = match error {
        DriverError::Unsupported(detail) | DriverError::Internal(detail) => detail.clone(),
        DriverError::OperationOutcomeUnknown { source } => match source.as_ref() {
            DriverError::Unsupported(detail) | DriverError::Internal(detail) => {
                crate::tr!("The operation was interrupted, but the database outcome could not be confirmed: {detail}")
                    .replace("{detail}", detail)
            }
            _ => return error_text::driver_message(error),
        },
        error => return error_text::driver_message(error),
    };
    append_missing_role_hint(text)
}

fn append_missing_role_hint(text: String) -> String {
    if !text.contains("role \"") || !text.contains("does not exist") {
        return text;
    }
    format!(
        "{text}\n\n{}",
        crate::tr!("Roles named in the backup must already exist on the target database.")
    )
}

fn archive_filter() -> gtk::FileFilter {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&crate::tr!("PostgreSQL backups")));
    filter.add_suffix("dump");
    filter
}

#[cfg(test)]
mod tests {
    use super::{append_missing_role_hint, backup_error_text};
    use tablepro_core::DriverError;

    #[test]
    fn a_restore_failure_from_pg_restore_omits_the_internal_driver_prefix() {
        let detail = "pg_restore failed with exit code 1: pg_restore: error: could not execute query: ERROR:  role \"comparison_analyst\" does not exist";
        let error = DriverError::OperationOutcomeUnknown {
            source: Box::new(DriverError::Internal(detail.into())),
        };
        let text = backup_error_text(&error);
        assert!(!text.contains("Internal driver error"), "{text}");
        assert!(text.contains("comparison_analyst"), "{text}");
        assert!(text.contains("does not exist"), "{text}");
        assert!(
            text.contains("Roles named in the backup must already exist on the target database."),
            "{text}"
        );
    }

    #[test]
    fn a_plain_internal_backup_error_keeps_the_tool_detail() {
        let detail = "pg_dump failed with exit code 1: permission denied";
        let text = backup_error_text(&DriverError::Internal(detail.into()));
        assert_eq!(text, detail);
        assert!(!text.contains("Internal driver error"));
    }

    #[test]
    fn a_failure_without_a_missing_role_gets_no_role_hint() {
        let text = append_missing_role_hint("pg_restore failed with exit code 1: disk full".into());
        assert_eq!(text, "pg_restore failed with exit code 1: disk full");
    }
}
