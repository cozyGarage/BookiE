use std::sync::{Arc, Mutex, PoisonError};

use bookie_app_services::database_service::DatabaseService;
use bookie_app_services::ssh_prompt::{ChannelHostKeyPrompter, PendingHostKey, Prompter};
use tokio::sync::mpsc::UnboundedSender;

static WINDOW: Mutex<Option<UnboundedSender<PendingHostKey>>> = Mutex::new(None);

pub fn install(tx: UnboundedSender<PendingHostKey>) {
    let mut window = WINDOW.lock().unwrap_or_else(PoisonError::into_inner);
    if window.as_ref().is_none_or(UnboundedSender::is_closed) {
        *window = Some(tx);
    }
}

pub fn prompter() -> Option<Arc<dyn Prompter>> {
    let window = WINDOW.lock().unwrap_or_else(PoisonError::into_inner);
    let tx = window.as_ref().filter(|tx| !tx.is_closed())?.clone();
    Some(Arc::new(ChannelHostKeyPrompter::new(tx)))
}

pub fn database_service() -> Arc<DatabaseService> {
    let database = DatabaseService::new();
    if let Some(prompter) = prompter() {
        database.enable_builtin_ssh_prompter(prompter);
    }
    Arc::new(database)
}
