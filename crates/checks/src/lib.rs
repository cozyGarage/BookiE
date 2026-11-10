use std::collections::VecDeque;
use std::time::{Duration, Instant};

use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Up { millis: u64 },
    Down { reason: String },
}

pub async fn check_tcp(address: &str, limit: Duration) -> Outcome {
    let started = Instant::now();
    match timeout(limit, TcpStream::connect(address)).await {
        Ok(Ok(_)) => Outcome::Up {
            millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        },
        Ok(Err(error)) => Outcome::Down {
            reason: error.to_string(),
        },
        Err(_) => Outcome::Down {
            reason: format!("no answer within {} ms", limit.as_millis()),
        },
    }
}

pub struct History {
    capacity: usize,
    outcomes: VecDeque<Outcome>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub checks: usize,
    pub up: usize,
    pub up_percent: f64,
    pub slowest_millis: Option<u64>,
    pub last: Option<Outcome>,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            outcomes: VecDeque::new(),
        }
    }

    pub fn record(&mut self, outcome: Outcome) {
        if self.outcomes.len() == self.capacity {
            self.outcomes.pop_front();
        }
        self.outcomes.push_back(outcome);
    }

    pub fn summary(&self) -> Summary {
        let checks = self.outcomes.len();
        let up = self.outcomes.iter().filter(|o| matches!(o, Outcome::Up { .. })).count();
        let slowest_millis = self
            .outcomes
            .iter()
            .filter_map(|o| match o {
                Outcome::Up { millis } => Some(*millis),
                Outcome::Down { .. } => None,
            })
            .max();
        Summary {
            checks,
            up,
            up_percent: if checks == 0 {
                0.0
            } else {
                up as f64 * 100.0 / checks as f64
            },
            slowest_millis,
            last: self.outcomes.back().cloned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn a_listening_port_is_up() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        assert!(matches!(
            check_tcp(&address, Duration::from_secs(2)).await,
            Outcome::Up { .. }
        ));
    }

    #[tokio::test]
    async fn a_closed_port_is_down_with_the_reason() {
        let address = {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            listener.local_addr().unwrap().to_string()
        };
        let Outcome::Down { reason } = check_tcp(&address, Duration::from_secs(2)).await else {
            panic!("nothing listens on {address}");
        };
        assert!(!reason.is_empty());
    }

    #[tokio::test]
    async fn an_address_that_does_not_answer_is_down() {
        let outcome = check_tcp("10.255.255.1:81", Duration::from_millis(50)).await;
        assert!(matches!(outcome, Outcome::Down { .. }), "{outcome:?}");
    }

    #[test]
    fn history_keeps_only_the_newest_checks_and_summarises_them() {
        let mut history = History::new(3);
        assert_eq!(history.summary().up_percent, 0.0);
        history.record(Outcome::Down { reason: "old".into() });
        history.record(Outcome::Up { millis: 5 });
        history.record(Outcome::Up { millis: 9 });
        history.record(Outcome::Down {
            reason: "refused".into(),
        });
        let summary = history.summary();
        assert_eq!(summary.checks, 3, "the oldest check was dropped");
        assert_eq!(summary.up, 2);
        assert_eq!(summary.slowest_millis, Some(9));
        assert_eq!(
            summary.last,
            Some(Outcome::Down {
                reason: "refused".into()
            })
        );
        assert!((summary.up_percent - 66.666).abs() < 0.01);
    }
}
