use async_trait::async_trait;
pub use bookie_ssh::openssh::{AskpassPrompt, PromptAnswer, Prompter};
use tokio::sync::{mpsc, oneshot};

pub struct PendingHostKey {
    pub host: String,
    pub algorithm: String,
    pub fingerprint: String,
    pub respond: oneshot::Sender<bool>,
}

pub type HostKeyInbox = mpsc::UnboundedReceiver<PendingHostKey>;

pub fn host_key_channel() -> (mpsc::UnboundedSender<PendingHostKey>, HostKeyInbox) {
    mpsc::unbounded_channel()
}

pub struct ChannelHostKeyPrompter {
    tx: mpsc::UnboundedSender<PendingHostKey>,
}

impl ChannelHostKeyPrompter {
    pub fn new(tx: mpsc::UnboundedSender<PendingHostKey>) -> Self {
        Self { tx }
    }
}

#[async_trait]
impl Prompter for ChannelHostKeyPrompter {
    async fn answer(&self, prompt: &AskpassPrompt) -> PromptAnswer {
        let AskpassPrompt::HostKeyConfirmation {
            host,
            algorithm,
            fingerprint,
        } = prompt
        else {
            return PromptAnswer::Decline;
        };
        let (respond, answer) = oneshot::channel();
        let pending = PendingHostKey {
            host: host.clone(),
            algorithm: algorithm.clone(),
            fingerprint: fingerprint.clone(),
            respond,
        };
        if self.tx.send(pending).is_err() {
            return PromptAnswer::Decline;
        }
        match answer.await {
            Ok(true) => PromptAnswer::Accept,
            _ => PromptAnswer::Decline,
        }
    }
}

#[cfg(test)]
#[path = "ssh_prompt_tests.rs"]
mod tests;
