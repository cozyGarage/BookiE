use std::io;
use std::sync::Arc;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use zeroize::Zeroizing;

use super::stderr_classify::DeclinedHostKey;
use super::{AskpassPrompt, OpenSshAuth, OpenSshConfig, PromptAnswer, Prompter};

pub(crate) const MAX_FRAME: usize = 65_536;
const FRAME_READ_TIMEOUT: Duration = Duration::from_secs(5);
const ANSWER: u8 = 0x00;
const CANCEL: u8 = 0x01;

pub(crate) struct AskpassBridge {
    owner_uid: u32,
    account: Option<(String, String)>,
    password: Option<SecretString>,
    key_path: Option<String>,
    passphrase: Option<SecretString>,
    prompter: Arc<dyn Prompter>,
    declined_host_key: Option<DeclinedHostKey>,
}

impl AskpassBridge {
    pub fn new(owner_uid: u32, config: &OpenSshConfig, prompter: Arc<dyn Prompter>) -> Self {
        let account = config
            .destination
            .user()
            .map(|user| (user.to_owned(), config.destination.host().to_owned()));
        let (password, key_path, passphrase) = match &config.auth {
            OpenSshAuth::Password { password } => (Some(password.clone()), None, None),
            OpenSshAuth::PrivateKey { path, passphrase } => (
                None,
                path.as_ref().map(|path| path.to_string_lossy().into_owned()),
                passphrase.clone(),
            ),
            OpenSshAuth::Agent => (None, None, None),
        };
        Self {
            owner_uid,
            account,
            password,
            key_path,
            passphrase,
            prompter,
            declined_host_key: None,
        }
    }

    pub fn declined_host_key(&self) -> Option<&DeclinedHostKey> {
        self.declined_host_key.as_ref()
    }

    pub async fn handle(&mut self, mut stream: UnixStream) -> io::Result<()> {
        if stream.peer_cred()?.uid() != self.owner_uid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the askpass peer belongs to another user",
            ));
        }
        let (hint, text) = tokio::time::timeout(FRAME_READ_TIMEOUT, read_request(&mut stream))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "the askpass helper sent no prompt"))??;
        let prompt = AskpassPrompt::classify(&String::from_utf8_lossy(&hint), &String::from_utf8_lossy(&text));
        let reply = self.reply_to(&prompt).await;
        stream.write_all(&reply).await?;
        stream.flush().await
    }

    async fn reply_to(&mut self, prompt: &AskpassPrompt) -> Zeroizing<Vec<u8>> {
        if let Some(stored) = self.stored_answer(prompt) {
            return stored;
        }
        if let AskpassPrompt::Notification { text } = prompt {
            self.prompter.notify(text);
            return answer_frame(b"");
        }
        match (prompt.is_confirmation(), self.prompter.answer(prompt).await) {
            (true, PromptAnswer::Accept) => answer_frame(b"yes"),
            (false, PromptAnswer::Secret(secret)) => answer_frame(secret.expose_secret().as_bytes()),
            _ => {
                self.record_declined(prompt);
                cancel_frame()
            }
        }
    }

    fn stored_answer(&mut self, prompt: &AskpassPrompt) -> Option<Zeroizing<Vec<u8>>> {
        let secret = match prompt {
            AskpassPrompt::Password { user_host } if self.is_configured_account(user_host) => self.password.take(),
            AskpassPrompt::Passphrase { key_path } if self.key_path.as_deref() == Some(key_path.as_str()) => {
                self.passphrase.take()
            }
            _ => None,
        }?;
        Some(answer_frame(secret.expose_secret().as_bytes()))
    }

    fn is_configured_account(&self, user_host: &str) -> bool {
        let Some((user, host)) = &self.account else {
            return false;
        };
        user_host
            .rsplit_once('@')
            .is_some_and(|(prompt_user, prompt_host)| prompt_user == user && prompt_host.eq_ignore_ascii_case(host))
    }

    fn record_declined(&mut self, prompt: &AskpassPrompt) {
        if let AskpassPrompt::HostKeyConfirmation {
            algorithm, fingerprint, ..
        } = prompt
        {
            self.declined_host_key = Some(DeclinedHostKey {
                algorithm: algorithm.clone(),
                fingerprint: fingerprint.clone(),
            });
        }
    }
}

async fn read_request(stream: &mut UnixStream) -> io::Result<(Vec<u8>, Vec<u8>)> {
    let hint = read_frame(stream).await?;
    let text = read_frame(stream).await?;
    Ok((hint, text))
}

async fn read_frame(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    let length = usize::try_from(stream.read_u32().await?).map_err(io::Error::other)?;
    if length > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the askpass frame is too large",
        ));
    }
    let mut frame = vec![0u8; length];
    stream.read_exact(&mut frame).await?;
    Ok(frame)
}

fn answer_frame(answer: &[u8]) -> Zeroizing<Vec<u8>> {
    let length = u32::try_from(answer.len() + 1).unwrap_or(u32::MAX);
    let mut frame = Zeroizing::new(Vec::with_capacity(answer.len() + 5));
    frame.extend_from_slice(&length.to_be_bytes());
    frame.push(ANSWER);
    frame.extend_from_slice(answer);
    frame
}

fn cancel_frame() -> Zeroizing<Vec<u8>> {
    let mut frame = Zeroizing::new(1u32.to_be_bytes().to_vec());
    frame.push(CANCEL);
    frame
}

#[cfg(test)]
#[path = "askpass_bridge_tests.rs"]
mod tests;
