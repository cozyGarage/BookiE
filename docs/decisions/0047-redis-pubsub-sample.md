# 0047: Redis pub/sub is read as a bounded sample

- **Status**: Proposed (2026-11-10)
- **Date**: 2026-11-10

## Context

The GPUI plan lists a pub/sub and stream monitor as a Phase 3 DBA module. A subscription is long-lived and push-based, which the request-and-response `Connection` trait does not model. A live monitor that never ends would also hold a connection and grow memory without a bound.

## Decision

1. The Redis driver crate exposes `sample_pubsub(options, patterns, max_messages, window)`. It opens its own subscriber connection, subscribes to the patterns with `PSUBSCRIBE`, and returns when it has `max_messages` messages or the window ends, whichever comes first.
2. Bounds are checked before connecting: 1 to 16 patterns of 1 to 256 characters, 1 to 1000 messages, a window up to 60 seconds. Each payload is cut at 4096 bytes and shows its real size.
3. Subscribing writes nothing, so it needs no write approval. It still uses the connection's TLS and credentials through the same client builder as `connect`.
4. A continuous monitor, `XREAD` stream tailing and Postgres LISTEN/NOTIFY (#2212) are not in this decision.
5. No frontend shows it yet.

## Consequences

A frontend can show "what is being published on these channels right now" without owning a subscription loop. Repeating the sample gives a refreshing view. Messages published between samples are not seen.
