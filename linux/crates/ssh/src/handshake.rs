use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub(super) enum HostKeyPromptEvent {
    WaitingForUser,
    UserResponded,
}

pub(super) enum HandshakeWaitError<E> {
    Timeout,
    Handshake(E),
}

pub(super) async fn wait_for_handshake<F, T, E>(
    handshake: F,
    mut prompt_events: tokio::sync::mpsc::UnboundedReceiver<HostKeyPromptEvent>,
    timeout: Duration,
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

    loop {
        tokio::select! {
            biased;
            Some(event) = prompt_events.recv() => match event {
                HostKeyPromptEvent::WaitingForUser if !waiting_for_user => {
                    remaining = deadline_at.saturating_duration_since(tokio::time::Instant::now());
                    waiting_for_user = true;
                }
                HostKeyPromptEvent::UserResponded => {
                    if waiting_for_user {
                        waiting_for_user = false;
                        deadline_at = tokio::time::Instant::now() + remaining;
                        deadline.as_mut().reset(deadline_at);
                    }
                }
                HostKeyPromptEvent::WaitingForUser => {}
            },
            result = &mut handshake => return result.map_err(HandshakeWaitError::Handshake),
            _ = &mut deadline, if !waiting_for_user => return Err(HandshakeWaitError::Timeout),
        }
    }
}
