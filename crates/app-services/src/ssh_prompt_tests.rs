use super::*;

fn host_key() -> AskpassPrompt {
    AskpassPrompt::HostKeyConfirmation {
        host: "db.example:22".into(),
        algorithm: "ssh-ed25519".into(),
        fingerprint: "SHA256:abc".into(),
    }
}

async fn answer_with(reply: Option<bool>) -> PromptAnswer {
    let (tx, mut inbox) = host_key_channel();
    let prompter = ChannelHostKeyPrompter::new(tx);
    let ui = tokio::spawn(async move {
        let pending = inbox.recv().await.expect("the prompt reached the inbox");
        assert_eq!(
            (
                pending.host.as_str(),
                pending.algorithm.as_str(),
                pending.fingerprint.as_str()
            ),
            ("db.example:22", "ssh-ed25519", "SHA256:abc")
        );
        if let Some(reply) = reply {
            let _ = pending.respond.send(reply);
        }
    });
    let answer = prompter.answer(&host_key()).await;
    ui.await.expect("ui task");
    answer
}

#[tokio::test]
async fn trusting_the_host_key_accepts_and_anything_else_declines() {
    assert!(matches!(answer_with(Some(true)).await, PromptAnswer::Accept));
    assert!(matches!(answer_with(Some(false)).await, PromptAnswer::Decline));
    assert!(matches!(answer_with(None).await, PromptAnswer::Decline));
}

#[tokio::test]
async fn without_a_listening_window_the_host_key_is_declined() {
    let (tx, inbox) = host_key_channel();
    drop(inbox);
    let answer = ChannelHostKeyPrompter::new(tx).answer(&host_key()).await;
    assert!(matches!(answer, PromptAnswer::Decline));
}

#[tokio::test]
async fn questions_other_than_a_host_key_are_declined_without_asking() {
    let (tx, mut inbox) = host_key_channel();
    let prompter = ChannelHostKeyPrompter::new(tx);
    let answer = prompter
        .answer(&AskpassPrompt::Password {
            user_host: "me@db".into(),
        })
        .await;
    assert!(matches!(answer, PromptAnswer::Decline));
    assert!(inbox.try_recv().is_err());
}
