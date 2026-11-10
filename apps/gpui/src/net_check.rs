use bookie_app_services::checks::{History, Outcome};

pub const HISTORY_CHECKS: usize = 20;

pub fn parse_address(text: &str) -> Option<String> {
    let text = text.trim();
    let (host, port) = text.rsplit_once(':')?;
    let valid_host = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    (valid_host && port.parse::<u16>().is_ok_and(|port| port > 0)).then(|| text.to_string())
}

pub fn kafka_topics(broker: &str) -> Result<String, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let topics = runtime
        .block_on(bookie_app_services::kafka_browser::list_topics(&[broker.to_string()]))
        .map_err(|error| error.to_string())?;
    if topics.is_empty() {
        return Ok(format!("{broker} · no topics"));
    }
    let names: Vec<String> = topics
        .iter()
        .map(|topic| {
            let retained: i64 = topic.partitions.iter().map(|p| p.retained()).sum();
            format!(
                "{} ({} partitions, {retained} records)",
                topic.name,
                topic.partitions.len()
            )
        })
        .collect();
    Ok(format!("{broker} · {}", names.join(" · ")))
}

pub fn run_check(address: &str) -> Outcome {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build();
    match runtime {
        Ok(runtime) => runtime.block_on(bookie_app_services::checks::check_tcp(
            address,
            std::time::Duration::from_secs(5),
        )),
        Err(error) => Outcome::Down {
            reason: error.to_string(),
        },
    }
}

pub fn record(histories: &mut Vec<(String, History)>, address: &str, outcome: Outcome) -> (bool, String) {
    let up = matches!(outcome, Outcome::Up { .. });
    let headline = match &outcome {
        Outcome::Up { millis } => format!("{address} is up · {millis} ms"),
        Outcome::Down { reason } => format!("{address} is down · {reason}"),
    };
    let index = match histories.iter().position(|(name, _)| name == address) {
        Some(index) => index,
        None => {
            histories.push((address.to_string(), History::new(HISTORY_CHECKS)));
            histories.len() - 1
        }
    };
    let history = &mut histories[index].1;
    history.record(outcome);
    let summary = history.summary();
    (up, format!("{headline} · up {}/{} checks", summary.up, summary.checks))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_host_and_port_make_an_address() {
        assert_eq!(parse_address(" db-1.internal:5432 "), Some("db-1.internal:5432".into()));
        for bad in ["", "db", "db:", ":5432", "db:0", "db:70000", "a b:1", "SELECT 1", "x:y"] {
            assert_eq!(parse_address(bad), None, "{bad}");
        }
    }

    #[test]
    fn repeated_checks_of_one_address_share_a_history() {
        let mut histories = Vec::new();
        let (up, first) = record(&mut histories, "h:1", Outcome::Up { millis: 3 });
        assert!(up);
        assert_eq!(first, "h:1 is up · 3 ms · up 1/1 checks");
        let (up, second) = record(
            &mut histories,
            "h:1",
            Outcome::Down {
                reason: "refused".into(),
            },
        );
        assert!(!up);
        assert_eq!(second, "h:1 is down · refused · up 1/2 checks");
        record(&mut histories, "other:2", Outcome::Up { millis: 1 });
        assert_eq!(histories.len(), 2);
    }

    #[test]
    fn a_listening_port_checks_up_and_a_closed_one_down() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        assert!(matches!(run_check(&address), Outcome::Up { .. }));
        drop(listener);
        assert!(matches!(run_check(&address), Outcome::Down { .. }));
    }
}

#[cfg(test)]
mod kafka_tests {
    #[test]
    fn a_broker_nobody_listens_on_is_reported_not_hung() {
        let error = super::kafka_topics("127.0.0.1:1").unwrap_err();
        assert!(error.contains("cannot reach the brokers"), "{error}");
    }
}
