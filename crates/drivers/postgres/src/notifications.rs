use std::time::Duration;

use bookie_core::{ConnectOptions, DriverError};
use sqlx::postgres::{PgListener, PgPoolOptions};

use crate::errors::map_sqlx_error;
use crate::full_connect_options;

pub const MAX_SAMPLE_MESSAGES: usize = 1000;
pub const MAX_SAMPLE_WINDOW: Duration = Duration::from_secs(60);
const MAX_CHANNELS: usize = 16;
const MAX_CHANNEL_BYTES: usize = 63;
const MAX_PAYLOAD_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationMessage {
    pub channel: String,
    pub payload: String,
    pub payload_bytes: usize,
    pub process_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationSample {
    pub messages: Vec<NotificationMessage>,
    pub reached_limit: bool,
}

pub fn check_notification_request(
    channels: &[String],
    max_messages: usize,
    window: Duration,
) -> Result<(), DriverError> {
    let bad = |text: &str| Err(DriverError::Unsupported(text.into()));
    if channels.is_empty() || channels.len() > MAX_CHANNELS {
        return bad("give between 1 and 16 channel names");
    }
    if channels
        .iter()
        .any(|channel| channel.is_empty() || channel.len() > MAX_CHANNEL_BYTES || channel.contains('\0'))
    {
        return bad("a channel name must be 1 to 63 bytes with no NUL");
    }
    if max_messages == 0 || max_messages > MAX_SAMPLE_MESSAGES {
        return bad("the message limit must be 1 to 1000");
    }
    if window.is_zero() || window > MAX_SAMPLE_WINDOW {
        return bad("the listening window must be up to 60 seconds");
    }
    Ok(())
}

fn to_message(channel: &str, payload: &str, process_id: u32) -> NotificationMessage {
    let mut end = payload.len().min(MAX_PAYLOAD_BYTES);
    while !payload.is_char_boundary(end) {
        end -= 1;
    }
    NotificationMessage {
        channel: channel.to_string(),
        payload: payload[..end].to_string(),
        payload_bytes: payload.len(),
        process_id,
    }
}

pub async fn follow_notifications(
    opts: ConnectOptions,
    channels: &[String],
    cancel: &tokio_util::sync::CancellationToken,
    mut on_message: impl FnMut(NotificationMessage) + Send,
) -> Result<u64, DriverError> {
    check_notification_request(channels, 1, Duration::from_secs(1))?;
    let pg_opts = full_connect_options(&opts)?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(bookie_core::CONTROL_SETUP_TIMEOUT)
        .connect_with(pg_opts)
        .await
        .map_err(map_sqlx_error)?;
    let mut listener = PgListener::connect_with(&pool).await.map_err(map_sqlx_error)?;
    listener
        .listen_all(channels.iter().map(String::as_str))
        .await
        .map_err(map_sqlx_error)?;
    let mut received = 0u64;
    let outcome = loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => break Ok(received),
            next = listener.try_recv() => match next {
                Ok(Some(notification)) => {
                    received += 1;
                    on_message(to_message(notification.channel(), notification.payload(), notification.process_id()));
                }
                Ok(None) => break Err(DriverError::Disconnected),
                Err(error) => break Err(map_sqlx_error(error)),
            },
        }
    };
    drop(listener);
    pool.close().await;
    outcome
}

pub async fn sample_notifications(
    opts: ConnectOptions,
    channels: &[String],
    max_messages: usize,
    window: Duration,
) -> Result<NotificationSample, DriverError> {
    check_notification_request(channels, max_messages, window)?;
    let pg_opts = full_connect_options(&opts)?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(bookie_core::CONTROL_SETUP_TIMEOUT)
        .connect_with(pg_opts)
        .await
        .map_err(map_sqlx_error)?;
    let mut listener = PgListener::connect_with(&pool).await.map_err(map_sqlx_error)?;
    listener
        .listen_all(channels.iter().map(String::as_str))
        .await
        .map_err(map_sqlx_error)?;
    let deadline = tokio::time::Instant::now() + window;
    let mut messages = Vec::new();
    while messages.len() < max_messages {
        match tokio::time::timeout_at(deadline, listener.recv()).await {
            Ok(Ok(notification)) => messages.push(to_message(
                notification.channel(),
                notification.payload(),
                notification.process_id(),
            )),
            Ok(Err(error)) => return Err(map_sqlx_error(error)),
            Err(_) => break,
        }
    }
    let reached_limit = messages.len() == max_messages;
    drop(listener);
    pool.close().await;
    Ok(NotificationSample {
        messages,
        reached_limit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn a_request_outside_the_bounds_is_refused_before_connecting() {
        let ok = Duration::from_secs(5);
        assert!(check_notification_request(&names(&["jobs"]), 10, ok).is_ok());
        assert!(check_notification_request(&[], 10, ok).is_err());
        assert!(check_notification_request(&names(&[""]), 10, ok).is_err());
        assert!(check_notification_request(&names(&[&"x".repeat(64)]), 10, ok).is_err());
        assert!(check_notification_request(&names(&["a\0b"]), 10, ok).is_err());
        assert!(check_notification_request(&names(&["a"; 17]), 10, ok).is_err());
        assert!(check_notification_request(&names(&["a"]), 0, ok).is_err());
        assert!(check_notification_request(&names(&["a"]), 1001, ok).is_err());
        assert!(check_notification_request(&names(&["a"]), 10, Duration::ZERO).is_err());
        assert!(check_notification_request(&names(&["a"]), 10, Duration::from_secs(61)).is_err());
    }

    #[test]
    fn a_long_payload_is_cut_on_a_character_boundary_and_reports_its_real_size() {
        let long = "é".repeat(MAX_PAYLOAD_BYTES);
        let message = to_message("jobs", &long, 7);
        assert!(message.payload.len() <= MAX_PAYLOAD_BYTES);
        assert!(message.payload.chars().all(|character| character == 'é'));
        assert_eq!(message.payload_bytes, long.len());
        assert_eq!(message.process_id, 7);
    }
}
