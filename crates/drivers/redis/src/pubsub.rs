use std::time::Duration;

use bookie_core::{ConnectOptions, DriverError};
use futures::StreamExt;

use crate::{CONNECT_TIMEOUT, build_client, map_redis_connect_error};

pub const MAX_SAMPLE_MESSAGES: usize = 1000;
pub const MAX_SAMPLE_WINDOW: Duration = Duration::from_secs(60);
const MAX_PATTERNS: usize = 16;
const MAX_PATTERN_CHARS: usize = 256;
const MAX_PAYLOAD_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PubSubMessage {
    pub channel: String,
    pub pattern: Option<String>,
    pub payload: String,
    pub payload_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PubSubSample {
    pub messages: Vec<PubSubMessage>,
    pub reached_limit: bool,
}

pub fn check_request(patterns: &[String], max_messages: usize, window: Duration) -> Result<(), DriverError> {
    let bad = |text: &str| Err(DriverError::Unsupported(text.into()));
    if patterns.is_empty() || patterns.len() > MAX_PATTERNS {
        return bad("give between 1 and 16 channel patterns");
    }
    if patterns
        .iter()
        .any(|p| p.is_empty() || p.chars().count() > MAX_PATTERN_CHARS)
    {
        return bad("a channel pattern must be 1 to 256 characters");
    }
    if max_messages == 0 || max_messages > MAX_SAMPLE_MESSAGES {
        return bad("the message limit must be 1 to 1000");
    }
    if window.is_zero() || window > MAX_SAMPLE_WINDOW {
        return bad("the listening window must be up to 60 seconds");
    }
    Ok(())
}

pub fn to_message(channel: &str, pattern: Option<String>, payload: &[u8]) -> PubSubMessage {
    let shown = &payload[..payload.len().min(MAX_PAYLOAD_BYTES)];
    PubSubMessage {
        channel: channel.to_string(),
        pattern,
        payload: String::from_utf8_lossy(shown).into_owned(),
        payload_bytes: payload.len(),
    }
}

pub async fn sample_pubsub(
    opts: ConnectOptions,
    patterns: &[String],
    max_messages: usize,
    window: Duration,
) -> Result<PubSubSample, DriverError> {
    check_request(patterns, max_messages, window)?;
    let verifies = opts.tls.mode.verifies_cert();
    let client = build_client(&opts)?;
    let mut pubsub = tokio::time::timeout(CONNECT_TIMEOUT, client.get_async_pubsub())
        .await
        .map_err(|_| DriverError::ConnectionRefused)?
        .map_err(|err| map_redis_connect_error(err, verifies))?;
    for pattern in patterns {
        pubsub
            .psubscribe(pattern.as_str())
            .await
            .map_err(|err| map_redis_connect_error(err, verifies))?;
    }
    let mut messages = Vec::new();
    let mut stream = pubsub.on_message();
    let deadline = tokio::time::Instant::now() + window;
    while messages.len() < max_messages {
        match tokio::time::timeout_at(deadline, stream.next()).await {
            Ok(Some(message)) => messages.push(to_message(
                message.get_channel_name(),
                message.get_pattern::<String>().ok(),
                message.get_payload_bytes(),
            )),
            Ok(None) | Err(_) => break,
        }
    }
    let reached_limit = messages.len() == max_messages;
    Ok(PubSubSample {
        messages,
        reached_limit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn a_request_outside_the_bounds_is_refused_before_connecting() {
        let ok = Duration::from_secs(5);
        assert!(check_request(&patterns(&["news.*"]), 10, ok).is_ok());
        assert!(check_request(&[], 10, ok).is_err());
        assert!(check_request(&patterns(&[""]), 10, ok).is_err());
        assert!(check_request(&patterns(&[&"x".repeat(257)]), 10, ok).is_err());
        assert!(check_request(&patterns(&["a"; 17]), 10, ok).is_err());
        assert!(check_request(&patterns(&["a"]), 0, ok).is_err());
        assert!(check_request(&patterns(&["a"]), 1001, ok).is_err());
        assert!(check_request(&patterns(&["a"]), 10, Duration::ZERO).is_err());
        assert!(check_request(&patterns(&["a"]), 10, Duration::from_secs(61)).is_err());
    }

    #[test]
    fn a_long_or_binary_payload_is_cut_and_reports_its_real_size() {
        let long = vec![b'x'; MAX_PAYLOAD_BYTES + 10];
        let message = to_message("c", None, &long);
        assert_eq!(message.payload.len(), MAX_PAYLOAD_BYTES);
        assert_eq!(message.payload_bytes, MAX_PAYLOAD_BYTES + 10);
        let binary = to_message("c", Some("p*".into()), &[0xff, b'a']);
        assert_eq!(binary.payload, "\u{fffd}a");
        assert_eq!(binary.pattern.as_deref(), Some("p*"));
    }
}
