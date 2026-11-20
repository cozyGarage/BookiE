use std::path::PathBuf;

use bookie_app_services::database_backup::BackupJob;
use bookie_core::export::normalize_duplicate_extension;
use bookie_gpui::live::WriteSession;

use super::*;
use crate::chrome::ghost;

const CONFIRM_WORD: &str = "RESTORE";

fn absolute_path(typed: &str) -> Result<PathBuf, String> {
    let path = bookie_core::user_path::expand_tilde(std::path::Path::new(typed));
    if path.is_absolute() {
        Ok(path)
    } else {
        Err("Use an absolute path, starting with / or ~/".into())
    }
}

pub(crate) fn backup_destination(typed: &str) -> Result<PathBuf, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err("Type where to save the backup".into());
    }
    let mut path = absolute_path(typed)?;
    if typed.ends_with('/') || path.file_name().is_none() {
        return Err("Type a file name for the backup, not a folder".into());
    }
    if path.extension().is_none() {
        path.set_extension("dump");
    }
    let path = normalize_duplicate_extension(path, "dump");
    if path.exists() {
        return Err("That file already exists; choose a new file name".into());
    }
    Ok(path)
}

pub(crate) fn restore_source(typed: &str) -> Result<PathBuf, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Err("Type the path of the backup to restore".into());
    }
    let path = absolute_path(typed)?;
    if path.is_file() {
        Ok(path)
    } else {
        Err("That backup file does not exist".into())
    }
}

pub(crate) fn job_for(restore: bool, typed: &str, confirmation: &str) -> Result<BackupJob, String> {
    if !restore {
        return backup_destination(typed).map(|output| BackupJob::Backup { output });
    }
    if !restore_confirmed(confirmation) {
        return Err(format!("Type {CONFIRM_WORD} in the confirmation field to restore"));
    }
    restore_source(typed).map(|input| BackupJob::Restore { input })
}

pub(crate) fn restore_confirmed(typed: &str) -> bool {
    typed.trim() == CONFIRM_WORD
}

#[derive(Default)]
pub(crate) struct BackupState {
    pub open: bool,
    pub restore: bool,
    pub focus: bool,
    pub running: bool,
    pub path: Option<Entity<TextInput>>,
    pub confirm: Option<Entity<TextInput>>,
}

fn run_job(
    session: &std::sync::Arc<bookie_gpui::live::LiveSession>,
    job: BackupJob,
    writable: Option<uuid::Uuid>,
    approvals: tokio::sync::mpsc::UnboundedSender<bookie_app_services::approval_prompt::PendingApproval>,
    opened_with: Option<String>,
) -> Result<(), String> {
    let never_cancelled = tokio_util::sync::CancellationToken::new();
    match (&job, writable) {
        (BackupJob::Restore { .. }, Some(connection_id)) => {
            let stamp = opened_with
                .as_deref()
                .ok_or("The saved connection could not be verified; reconnect and try again")?;
            WriteSession::open_saved(connection_id, approvals, Some(stamp))?.run_backup(job, &never_cancelled)
        }
        (BackupJob::Restore { .. }, None) => Err("This connection is read-only, so it cannot be restored".into()),
        _ => session.run_backup(job, &never_cancelled),
    }
}

impl GridView {
    fn backup_supported(&self) -> bool {
        self.live
            .as_ref()
            .is_some_and(|session| session.driver_id == "postgres")
    }

    pub(crate) fn backup_actions(&self) -> Vec<Action> {
        let supported = self.backup_supported();
        let restorable = supported && self.live.as_ref().is_some_and(|session| session.writable.is_some());
        vec![
            Action {
                id: "backup-database",
                label: "Back up the database to a file…".to_string(),
                enabled: supported,
            },
            Action {
                id: "restore-database",
                label: "Restore the database from a backup…".to_string(),
                enabled: restorable,
            },
        ]
    }

    pub(crate) fn focus_backup_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.search.backup.focus) {
            return;
        }
        if let Some(input) = &self.search.backup.path {
            window.focus(&input.read(cx).focus_handle(cx), cx);
        }
    }

    fn backup_input(&mut self, hint: &str, cx: &mut Context<Self>) -> Entity<TextInput> {
        let input = cx.new(|cx| TextInput::new(hint, cx));
        cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Submit) {
                this.start_backup_job(cx);
            }
        })
        .detach();
        input
    }

    pub(crate) fn open_backup_bar(&mut self, restore: bool, cx: &mut Context<Self>) {
        let writable = self.live.as_ref().is_some_and(|session| session.writable.is_some());
        if !self.backup_supported() || (restore && !writable) {
            self.status =
                "Backups are available for PostgreSQL connections; a restore also needs a writable one".into();
            return;
        }
        if self.search.backup.path.is_none() {
            let path = self.backup_input("Path of the .dump file", cx);
            let confirm = self.backup_input(&format!("Type {CONFIRM_WORD} to confirm"), cx);
            self.search.backup.path = Some(path);
            self.search.backup.confirm = Some(confirm);
        }
        for input in [self.search.backup.path.clone(), self.search.backup.confirm.clone()]
            .into_iter()
            .flatten()
        {
            input.update(cx, |input, cx| input.set_content("", cx));
        }
        self.search.backup.open = true;
        self.search.backup.restore = restore;
        self.search.backup.focus = true;
        self.status = if restore {
            "A restore replaces objects in this database; type the backup path and the confirmation word".into()
        } else {
            "Type where to save the backup of this database".into()
        };
        cx.notify();
    }

    pub(crate) fn start_backup_job(&mut self, cx: &mut Context<Self>) {
        let (Some(session), Some(path), Some(confirm)) = (
            self.live.clone(),
            self.search.backup.path.clone(),
            self.search.backup.confirm.clone(),
        ) else {
            return;
        };
        if self.search.backup.running {
            self.status = "A backup or restore is already running".into();
            cx.notify();
            return;
        }
        let restore = self.search.backup.restore;
        let approvals = self.approval.tx.clone();
        let opened_with = session.stamp.clone();
        let writable = session.writable;
        let typed = path.read(cx).content().to_string();
        let job = job_for(restore, &typed, confirm.read(cx).content().as_ref());
        let job = match job {
            Ok(job) => job,
            Err(message) => {
                self.status = message;
                cx.notify();
                return;
            }
        };
        self.search.backup.open = false;
        self.status = if restore { "Restoring…" } else { "Backing up…" }.into();
        let shown = match &job {
            BackupJob::Backup { output } => output.display().to_string(),
            BackupJob::Restore { input } => input.display().to_string(),
        };
        self.search.backup.running = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { run_job(&session, job, writable, approvals, opened_with) })
                .await;
            this.update(cx, |this, cx| {
                this.search.backup.running = false;
                this.status = match result {
                    Ok(()) if restore => {
                        "The database was restored from the backup; reopen tables to see the changes".into()
                    }
                    Ok(()) => format!("The backup was saved to {shown}"),
                    Err(message) => message,
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn backup_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<Div> {
        let (path, confirm) = (self.search.backup.path.clone()?, self.search.backup.confirm.clone()?);
        if !self.search.backup.open {
            return None;
        }
        let restore = self.search.backup.restore;
        let mut bar = div()
            .flex()
            .items_center()
            .gap_3()
            .p_2()
            .flex_none()
            .bg(rgb(theme.chrome))
            .border_b_1()
            .border_color(rgb(theme.border))
            .child(div().font_weight(gpui::FontWeight::MEDIUM).child(if restore {
                "Restore database"
            } else {
                "Back up database"
            }))
            .child(div().flex_1().min_w_0().child(path));
        if restore {
            bar = bar.child(div().w(px(size::FORM_CONTROL * 5.0)).child(confirm));
        }
        Some(
            bar.child(
                ghost("run-backup-job", if restore { "Restore" } else { "Back up" }, theme)
                    .on_click(cx.listener(|this, _, _, cx| this.start_backup_job(cx))),
            )
            .child(
                ghost("cancel-backup-job", "Cancel", theme).on_click(cx.listener(|this, _, _, cx| {
                    this.search.backup.open = false;
                    cx.notify();
                })),
            ),
        )
    }
}

#[cfg(test)]
#[path = "backup_ui_tests.rs"]
mod tests;
