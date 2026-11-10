# 0050: Avro object container files are decoded with their own schema

- **Status**: Proposed (2026-11-10)
- **Date**: 2026-11-10

## Context

The GPUI plan lists Avro in the decoder work. An Avro datum on its own cannot be decoded without its writer's schema. An object container file carries that schema in its header, so a file can be decoded without any outside input.

## Decision

1. `bookie-avro-container` decodes an Avro object container file with `apache-avro` (Apache-2.0, a pure-Rust reader), returning at most 1000 records as JSON values. Union values unwrap to their inner value; a byte string or fixed value shows its debug text, not raw bytes.
2. Files over 16 MiB and bytes that are not a container are refused, not guessed.
3. Raw Avro datums, files written without a container, and schema registry lookups are out of scope; each needs its own writer's schema, which no cell carries today.
4. No frontend shows it yet. It depends on no other workspace crate.

## Consequences

A file dropped in the app can be read without a schema. Datum-level decoding waits for a place that holds the schema.
