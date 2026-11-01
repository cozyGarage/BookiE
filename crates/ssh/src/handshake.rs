use std::time::Duration;

pub(crate) const PROMPT_WAIT_LIMIT: Duration = Duration::from_secs(300);

#[derive(Debug)]
pub(super) enum HostKeyPromptEvent {
    WaitingForUser,
    UserResponded { declined_fingerprint: Option<String> },
}

pub(super) enum HandshakeWaitError<E> {
    Timeout,
    HostKeyDeclined(String),
    Handshake(E),
}

pub(super) async fn wait_for_handshake<F, T, E>(
    handshake: F,
    prompt_events: tokio::sync::mpsc::UnboundedReceiver<HostKeyPromptEvent>,
    timeout: Duration,
) -> Result<T, HandshakeWaitError<E>>
where
    F: std::future::Future<Output = Result<T, E>>,
{
    wait_for_handshake_limiting_prompts(handshake, prompt_events, timeout, PROMPT_WAIT_LIMIT).await
}

async fn wait_for_handshake_limiting_prompts<F, T, E>(
    handshake: F,
    mut prompt_events: tokio::sync::mpsc::UnboundedReceiver<HostKeyPromptEvent>,
    timeout: Duration,
    prompt_limit: Duration,
) -> Result<T, HandshakeWaitError<E>>
where
    F: std::future::Future<Output = Result<T, E>>,
{
    let mut handshake = std::pin::pin!(handshake);
    let mut deadline_at = tokio::time::Instant::now() + timeout;
    let deadline = tokio::time::sleep_until(deadline_at);
    tokio::pin!(deadline);
    let mut waiting_for_user = false;
    let mut remaining = timeout;
    let prompt_deadline = tokio::time::sleep(prompt_limit);
    tokio::pin!(prompt_deadline);

    loop {
        tokio::select! {
            biased;
            Some(event) = prompt_events.recv() => match event {
                HostKeyPromptEvent::WaitingForUser if !waiting_for_user => {
                    remaining = deadline_at.saturating_duration_since(tokio::time::Instant::now());
                    waiting_for_user = true;
                    prompt_deadline.as_mut().reset(tokio::time::Instant::now() + prompt_limit);
                }
                HostKeyPromptEvent::UserResponded { declined_fingerprint } => {
                    if waiting_for_user {
                        waiting_for_user = false;
                        if let Some(fingerprint) = declined_fingerprint {
                            tracing::warn!(%fingerprint, "SSH host key declined; ending handshake");
                            return Err(HandshakeWaitError::HostKeyDeclined(fingerprint));
                        }
                        deadline_at = tokio::time::Instant::now() + remaining;
                        deadline.as_mut().reset(deadline_at);
                    }
                }
                HostKeyPromptEvent::WaitingForUser => {}
            },
            result = &mut handshake => return result.map_err(HandshakeWaitError::Handshake),
            _ = &mut deadline, if !waiting_for_user => return Err(HandshakeWaitError::Timeout),
            _ = &mut prompt_deadline, if waiting_for_user => return Err(HandshakeWaitError::Timeout),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn declining_a_prompt_ends_a_pending_ssh_handshake_immediately() {
        let (events, receiver) = tokio::sync::mpsc::unbounded_channel();
        let handshake = async move {
            events.send(HostKeyPromptEvent::WaitingForUser).unwrap();
            events
                .send(HostKeyPromptEvent::UserResponded {
                    declined_fingerprint: Some("SHA256:declined".to_owned()),
                })
                .unwrap();
            std::future::pending::<Result<(), ()>>().await
        };

        assert!(matches!(
            wait_for_handshake(handshake, receiver, Duration::from_secs(10)).await,
            Err(HandshakeWaitError::HostKeyDeclined(fingerprint)) if fingerprint == "SHA256:declined"
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn an_unanswered_prompt_ends_the_handshake_after_the_prompt_limit() {
        let (events, receiver) = tokio::sync::mpsc::unbounded_channel();
        let handshake = async move {
            events.send(HostKeyPromptEvent::WaitingForUser).unwrap();
            std::future::pending::<Result<(), ()>>().await
        };
        let started = tokio::time::Instant::now();

        let result =
            wait_for_handshake_limiting_prompts(handshake, receiver, Duration::from_secs(10), Duration::from_secs(300))
                .await;

        assert!(matches!(result, Err(HandshakeWaitError::Timeout)));
        assert_eq!(started.elapsed(), Duration::from_secs(300));
    }

    #[tokio::test(start_paused = true)]
    async fn an_answered_prompt_does_not_trip_the_prompt_limit() {
        let (events, receiver) = tokio::sync::mpsc::unbounded_channel();
        let handshake = async move {
            events.send(HostKeyPromptEvent::WaitingForUser).unwrap();
            tokio::time::sleep(Duration::from_secs(200)).await;
            events
                .send(HostKeyPromptEvent::UserResponded {
                    declined_fingerprint: None,
                })
                .unwrap();
            tokio::time::sleep(Duration::from_secs(3)).await;
            Ok::<_, ()>("connected")
        };

        let result =
            wait_for_handshake_limiting_prompts(handshake, receiver, Duration::from_secs(10), Duration::from_secs(300))
                .await;

        assert!(matches!(result, Ok("connected")));
    }
}
