use async_trait::async_trait;
use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use secrecy::SecretString;
use tablepro_ssh::openssh::{AskpassPrompt, PromptAnswer, Prompter};

pub(crate) struct GtkPrompter;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Reply {
    Confirm,
    Secret,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Question {
    heading: String,
    body: String,
    accept: String,
    reply: Reply,
}

#[async_trait]
impl Prompter for GtkPrompter {
    async fn answer(&self, prompt: &AskpassPrompt) -> PromptAnswer {
        let Some(question) = question_for(prompt) else {
            return PromptAnswer::Decline;
        };
        let (sender, receiver) = tokio::sync::oneshot::channel();
        glib::MainContext::default().invoke(move || ask(question, sender));
        receiver.await.unwrap_or(PromptAnswer::Decline)
    }

    fn notify(&self, text: &str) {
        tracing::info!(message = %text, "ssh notice");
    }
}

fn question_for(prompt: &AskpassPrompt) -> Option<Question> {
    let (heading, body, accept, reply) = match prompt {
        AskpassPrompt::HostKeyConfirmation {
            host,
            algorithm,
            fingerprint,
        } => (
            crate::tr!("Trust this SSH host?"),
            crate::tr!(
                "This computer has not connected to {host} before. Its {algorithm} key fingerprint is {fingerprint}. Compare it with the fingerprint your server administrator gave you before trusting it."
            )
            .replace("{host}", host)
            .replace("{algorithm}", algorithm)
            .replace("{fingerprint}", fingerprint),
            crate::tr!("Trust"),
            Reply::Confirm,
        ),
        AskpassPrompt::Confirmation { text } => (crate::tr!("Confirm SSH request"), text.clone(), crate::tr!("Allow"), Reply::Confirm),
        AskpassPrompt::Password { user_host } => (
            crate::tr!("SSH password"),
            crate::tr!("Enter the password for {account}.").replace("{account}", user_host),
            crate::tr!("Connect"),
            Reply::Secret,
        ),
        AskpassPrompt::Passphrase { key_path } => (
            crate::tr!("SSH key passphrase"),
            crate::tr!("Enter the passphrase for {key}.").replace("{key}", key_path),
            crate::tr!("Unlock"),
            Reply::Secret,
        ),
        AskpassPrompt::KeyboardInteractive { text } | AskpassPrompt::Other { text } => {
            (crate::tr!("SSH server question"), text.clone(), crate::tr!("Send"), Reply::Secret)
        }
        AskpassPrompt::Notification { .. } => return None,
    };
    Some(Question {
        heading,
        body,
        accept,
        reply,
    })
}

fn ask(question: Question, sender: tokio::sync::oneshot::Sender<PromptAnswer>) {
    let dialog = adw::AlertDialog::new(Some(&question.heading), Some(&question.body));
    dialog.add_response("cancel", &crate::tr!("Cancel"));
    dialog.add_response("accept", &question.accept);
    dialog.set_default_response(Some(if question.reply == Reply::Secret {
        "accept"
    } else {
        "cancel"
    }));
    dialog.set_close_response("cancel");
    let entry = (question.reply == Reply::Secret).then(|| {
        let entry = gtk::PasswordEntry::builder()
            .activates_default(true)
            .show_peek_icon(true)
            .build();
        dialog.set_extra_child(Some(&entry));
        entry
    });
    let sender = std::cell::RefCell::new(Some(sender));
    dialog.connect_response(None, move |_, response| {
        let answer = match (response, &entry) {
            ("accept", Some(entry)) => PromptAnswer::Secret(SecretString::new(entry.text().to_string().into())),
            ("accept", None) => PromptAnswer::Accept,
            _ => PromptAnswer::Decline,
        };
        if let Some(sender) = sender.borrow_mut().take() {
            let _ = sender.send(answer);
        }
    });
    let parent = gtk::Application::default().active_window();
    dialog.present(parent.as_ref());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_key_question_names_the_host_and_fingerprint_and_asks_for_confirmation() {
        let question = question_for(&AskpassPrompt::HostKeyConfirmation {
            host: "bastion.example".into(),
            algorithm: "ED25519".into(),
            fingerprint: "SHA256:abc".into(),
        })
        .unwrap();

        assert_eq!(question.reply, Reply::Confirm);
        assert!(question.body.contains("bastion.example"));
        assert!(question.body.contains("SHA256:abc"));
    }

    #[test]
    fn secrets_are_asked_for_by_account_or_key_and_notices_raise_no_dialog() {
        let password = question_for(&AskpassPrompt::Password {
            user_host: "deploy@db".into(),
        })
        .unwrap();
        assert_eq!(password.reply, Reply::Secret);
        assert!(password.body.contains("deploy@db"));
        assert!(question_for(&AskpassPrompt::Notification { text: "banner".into() }).is_none());
    }
}
