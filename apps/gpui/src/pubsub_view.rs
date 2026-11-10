use drivers_registry::PubSubSample;

use crate::messages::MessageKind;

pub const SAMPLE_MESSAGES: usize = 50;
pub const SAMPLE_SECONDS: u64 = 5;
const SHOWN_PAYLOAD_CHARS: usize = 200;

pub fn parse_pattern(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty() && text.chars().count() <= 256 && !text.contains(char::is_whitespace)).then(|| text.to_string())
}

pub fn describe(pattern: &str, outcome: Result<PubSubSample, String>) -> Vec<(MessageKind, String)> {
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
        let payload: String = message.payload.chars().take(SHOWN_PAYLOAD_CHARS).collect();
        let cut =
            if message.payload_bytes > message.payload.len() || message.payload.chars().count() > SHOWN_PAYLOAD_CHARS {
                " …"
            } else {
                ""
            };
        lines.push((MessageKind::Info, format!("{} · {payload}{cut}", message.channel)));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use drivers_registry::PubSubMessage;

    fn message(channel: &str, payload: &str, bytes: usize) -> PubSubMessage {
        PubSubMessage {
            channel: channel.into(),
            pattern: Some("news.*".into()),
            payload: payload.into(),
            payload_bytes: bytes,
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
        let sample = PubSubSample {
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
        let full = PubSubSample {
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
