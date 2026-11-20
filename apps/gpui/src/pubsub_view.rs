use bookie_app_services::read_session_store::SubscriptionSample;

use crate::messages::MessageKind;

pub const SAMPLE_MESSAGES: usize = 50;
pub const SAMPLE_SECONDS: u64 = 5;
const SHOWN_PAYLOAD_CHARS: usize = 200;

pub fn parse_pattern(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty() && text.chars().count() <= 256 && !text.contains(char::is_whitespace)).then(|| text.to_string())
}

fn channel_after(text: &str, word: &str) -> Option<String> {
    let text = text.trim();
    let prefix = word.len() + 1;
    let name = text
        .get(..prefix)
        .filter(|head| head[..word.len()].eq_ignore_ascii_case(word) && head.ends_with(' '))
        .and_then(|_| text.get(prefix..))?
        .trim();
    (!name.is_empty() && name.len() <= 63 && !name.contains(char::is_whitespace)).then(|| name.to_string())
}

pub fn parse_channel(text: &str) -> Option<String> {
    channel_after(text, "listen")
}

pub fn parse_follow(text: &str) -> Option<String> {
    channel_after(text, "follow")
}

pub fn message_line(message: &bookie_app_services::read_session_store::SubscriptionMessage) -> String {
    let payload: String = message.payload.chars().take(SHOWN_PAYLOAD_CHARS).collect();
    let cut =
        if message.payload_bytes > message.payload.len() || message.payload.chars().count() > SHOWN_PAYLOAD_CHARS {
            " …"
        } else {
            ""
        };
    format!("{} · {payload}{cut}", message.channel)
}

pub fn describe(pattern: &str, outcome: Result<SubscriptionSample, String>) -> Vec<(MessageKind, String)> {
    let sample = match outcome {
        Ok(sample) => sample,
        Err(error) => return vec![(MessageKind::Error, format!("{pattern} · could not listen · {error}"))],
    };
    let count = sample.messages.len();
    let ending = if sample.reached_limit {
        format!("stopped at the {SAMPLE_MESSAGES}-message limit")
    } else {
        format!("listened {SAMPLE_SECONDS} s")
    };
    let mut lines = vec![(MessageKind::Success, format!("{pattern} · {count} messages · {ending}"))];
    for message in sample.messages {
        lines.push((MessageKind::Info, message_line(&message)));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_app_services::read_session_store::SubscriptionMessage;

    fn message(channel: &str, payload: &str, bytes: usize) -> SubscriptionMessage {
        SubscriptionMessage {
            channel: channel.into(),
            pattern: Some("news.*".into()),
            payload: payload.into(),
            payload_bytes: bytes,
        }
    }

    #[test]
    fn follow_takes_the_follow_word_and_one_short_name() {
        assert_eq!(parse_follow("follow jobs"), Some("jobs".into()));
        assert_eq!(parse_follow("  FOLLOW   orders_changed "), Some("orders_changed".into()));
        for bad in ["jobs", "follow", "follow ", "follow two words", "listen jobs", "unfollow jobs"] {
            assert_eq!(parse_follow(bad), None, "{bad}");
        }
        assert_eq!(parse_channel("follow jobs"), None, "listen and follow are different commands");
    }

    #[test]
    fn a_postgres_channel_needs_the_listen_word_and_one_short_name() {
        assert_eq!(parse_channel("listen jobs"), Some("jobs".into()));
        assert_eq!(
            parse_channel("  LISTEN   orders_changed "),
            Some("orders_changed".into())
        );
        for bad in [
            "jobs",
            "listen",
            "listen ",
            "listen two words",
            "unlisten jobs",
            &format!("listen {}", "x".repeat(64)),
        ] {
            assert_eq!(parse_channel(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_pattern_is_one_word_of_reasonable_length() {
        assert_eq!(parse_pattern(" news.* "), Some("news.*".into()));
        for bad in ["", "   ", "two words", &"x".repeat(257)] {
            assert_eq!(parse_pattern(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_sample_becomes_a_headline_and_one_line_per_message() {
        let sample = SubscriptionSample {
            messages: vec![message("news.1", "hello", 5), message("news.2", "cut", 9000)],
            reached_limit: false,
        };
        let lines = describe("news.*", Ok(sample));
        assert_eq!(
            lines[0],
            (MessageKind::Success, "news.* · 2 messages · listened 5 s".into())
        );
        assert_eq!(lines[1], (MessageKind::Info, "news.1 · hello".into()));
        assert_eq!(
            lines[2],
            (MessageKind::Info, "news.2 · cut …".into()),
            "a cut payload says so"
        );
    }

    #[test]
    fn hitting_the_limit_and_failing_are_both_reported() {
        let full = SubscriptionSample {
            messages: vec![message("a", "x", 1)],
            reached_limit: true,
        };
        assert!(
            describe("a", Ok(full))[0]
                .1
                .ends_with("stopped at the 50-message limit")
        );
        let failed = describe("a", Err("Disconnected".into()));
        assert_eq!(
            failed,
            vec![(MessageKind::Error, "a · could not listen · Disconnected".into())]
        );
    }
}
