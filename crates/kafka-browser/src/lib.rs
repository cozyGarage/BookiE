use std::time::Duration;

use rskafka::client::partition::{OffsetAt, UnknownTopicHandling};
use rskafka::client::{Client, ClientBuilder};

pub const MAX_TAIL_RECORDS: usize = 200;
const MAX_VALUE_BYTES: usize = 4096;
const FETCH_BYTES: std::ops::Range<i32> = 1..4_000_000;
const FETCH_WAIT_MS: i32 = 500;
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KafkaError {
    Unreachable(String),
    Refused(String),
    Request(String),
}

impl std::fmt::Display for KafkaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(text) => write!(f, "cannot reach the brokers: {text}"),
            Self::Refused(text) => write!(f, "refused: {text}"),
            Self::Request(text) => write!(f, "{text}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionSummary {
    pub id: i32,
    pub earliest: i64,
    pub latest: i64,
}

impl PartitionSummary {
    pub fn retained(&self) -> i64 {
        (self.latest - self.earliest).max(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicSummary {
    pub name: String,
    pub partitions: Vec<PartitionSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordView {
    pub partition: i32,
    pub offset: i64,
    pub timestamp_ms: i64,
    pub key: Option<String>,
    pub value: String,
    pub value_bytes: usize,
}

async fn client(brokers: &[String]) -> Result<Client, KafkaError> {
    if brokers.is_empty() || brokers.iter().any(|b| b.trim().is_empty()) {
        return Err(KafkaError::Request("give at least one broker as host:port".into()));
    }
    let builder = ClientBuilder::new(brokers.to_vec()).client_id("bookie");
    match tokio::time::timeout(CONNECT_TIMEOUT, builder.build()).await {
        Ok(Ok(client)) => Ok(client),
        Ok(Err(error)) => Err(KafkaError::Unreachable(error.to_string())),
        Err(_) => Err(KafkaError::Unreachable("no answer within 10 seconds".into())),
    }
}

fn request_error(error: rskafka::client::error::Error) -> KafkaError {
    let text = error.to_string();
    if text.contains("Authorization") || text.contains("SaslAuthentication") {
        KafkaError::Refused(text)
    } else {
        KafkaError::Request(text)
    }
}

pub async fn list_topics(brokers: &[String]) -> Result<Vec<TopicSummary>, KafkaError> {
    let client = client(brokers).await?;
    let mut topics = client.list_topics().await.map_err(request_error)?;
    topics.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out = Vec::with_capacity(topics.len());
    for topic in topics {
        let mut partitions = Vec::new();
        for id in topic.partitions {
            let partition = client
                .partition_client(topic.name.clone(), id, UnknownTopicHandling::Error)
                .await
                .map_err(request_error)?;
            let earliest = partition.get_offset(OffsetAt::Earliest).await.map_err(request_error)?;
            let latest = partition.get_offset(OffsetAt::Latest).await.map_err(request_error)?;
            partitions.push(PartitionSummary { id, earliest, latest });
        }
        out.push(TopicSummary {
            name: topic.name,
            partitions,
        });
    }
    Ok(out)
}

pub fn first_offset_to_read(earliest: i64, latest: i64, count: usize) -> i64 {
    let count = i64::try_from(count.clamp(1, MAX_TAIL_RECORDS)).unwrap_or(1);
    (latest - count).max(earliest)
}

pub fn view(partition: i32, offset: i64, record: &rskafka::record::Record) -> RecordView {
    let value = record.value.as_deref().unwrap_or_default();
    let shown = &value[..value.len().min(MAX_VALUE_BYTES)];
    RecordView {
        partition,
        offset,
        timestamp_ms: record.timestamp.timestamp_millis(),
        key: record
            .key
            .as_deref()
            .map(|key| String::from_utf8_lossy(key).into_owned()),
        value: String::from_utf8_lossy(shown).into_owned(),
        value_bytes: value.len(),
    }
}

pub async fn tail(
    brokers: &[String],
    topic: &str,
    partition: i32,
    count: usize,
) -> Result<Vec<RecordView>, KafkaError> {
    let client = client(brokers).await?;
    let partition_client = client
        .partition_client(topic.to_string(), partition, UnknownTopicHandling::Error)
        .await
        .map_err(request_error)?;
    let earliest = partition_client
        .get_offset(OffsetAt::Earliest)
        .await
        .map_err(request_error)?;
    let latest = partition_client
        .get_offset(OffsetAt::Latest)
        .await
        .map_err(request_error)?;
    if latest <= earliest {
        return Ok(Vec::new());
    }
    let mut next = first_offset_to_read(earliest, latest, count);
    let mut out = Vec::new();
    while next < latest && out.len() < MAX_TAIL_RECORDS {
        let (records, _high) = partition_client
            .fetch_records(next, FETCH_BYTES, FETCH_WAIT_MS)
            .await
            .map_err(request_error)?;
        if records.is_empty() {
            break;
        }
        for item in records {
            next = item.offset + 1;
            if item.offset < latest && out.len() < MAX_TAIL_RECORDS {
                out.push(view(partition, item.offset, &item.record));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tail_starts_count_records_before_the_end_but_never_before_the_start() {
        assert_eq!(first_offset_to_read(0, 100, 10), 90);
        assert_eq!(
            first_offset_to_read(95, 100, 10),
            95,
            "retention already dropped the older ones"
        );
        assert_eq!(first_offset_to_read(0, 1000, 5000), 800, "count is capped at 200");
        assert_eq!(first_offset_to_read(0, 5, 0), 4, "zero still reads one");
    }

    #[test]
    fn a_record_shows_its_key_and_a_cut_value_with_the_real_size() {
        let record = rskafka::record::Record {
            key: Some(b"user-1".to_vec()),
            value: Some(vec![b'x'; MAX_VALUE_BYTES + 5]),
            headers: Default::default(),
            timestamp: chrono_epoch(),
        };
        let shown = view(2, 41, &record);
        assert_eq!((shown.partition, shown.offset), (2, 41));
        assert_eq!(shown.key.as_deref(), Some("user-1"));
        assert_eq!(shown.value.len(), MAX_VALUE_BYTES);
        assert_eq!(shown.value_bytes, MAX_VALUE_BYTES + 5);
        let empty = rskafka::record::Record {
            key: None,
            value: None,
            headers: Default::default(),
            timestamp: chrono_epoch(),
        };
        assert_eq!(view(0, 0, &empty).value, "", "a tombstone has no value");
    }

    fn chrono_epoch() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(0, 0).expect("epoch")
    }

    #[tokio::test]
    async fn no_brokers_is_refused_before_connecting() {
        assert!(matches!(list_topics(&[]).await, Err(KafkaError::Request(_))));
        assert!(matches!(list_topics(&[" ".into()]).await, Err(KafkaError::Request(_))));
    }
}
