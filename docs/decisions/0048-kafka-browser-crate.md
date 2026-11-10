# 0048: Kafka topics are browsed through a UI-free crate on a pure-Rust client

- **Status**: Proposed (2026-11-10)
- **Date**: 2026-11-10

## Context

The GPUI plan lists a message-queue browser as a Phase 3 DBA module and leaves the broker open. The maintainer suggested Kafka. A Kafka client that links `librdkafka` adds a C build dependency to every contributor machine and CI runner.

## Decision

1. `bookie-kafka-browser` holds the logic: `list_topics(brokers)` returns each topic's partitions with earliest and latest offsets, and `tail(brokers, topic, partition, count)` returns the newest records (up to 200) with offset, timestamp, key and value. Values are cut at 4096 bytes and keep their real size; a tombstone has an empty value.
2. It uses the pure-Rust `rskafka` client (MIT OR Apache-2.0) with gzip, lz4 and snappy decompression and no TLS. Its zstd feature is off because `zstd 0.13` and `zstd-safe 7.3` carry open advisories (RUSTSEC-2026-0391, -0398, -0403) that `cargo deny` rejects, so a zstd-compressed topic fails with a decode error until a fixed release exists.
3. It reads only. Producing, deleting records, committing offsets and consumer-group management are not in this decision.
4. It is not yet an audited operation: a frontend must call it through a `PolicyGuard` operation like ADR 0047 before it ships, and broker credentials (SASL, TLS) need a saved-connection shape. Both are separate decisions; this crate accepts plain `host:port` brokers only, with no TLS.
5. It depends on no other workspace crate. No frontend shows it yet.

## Consequences

The browser logic can be tested against a real broker in a container (it is, with Apache Kafka in KRaft mode). Until step 4 is decided it must not be wired to a saved connection.
