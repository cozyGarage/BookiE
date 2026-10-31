# 0017: Adopt the official Rust MCP SDK behind the governed seam

- **Status**: Accepted
- **Date**: 2026-10-12

## Context

The MCP server implements protocol framing, version negotiation, transport handling, tool descriptions and JSON schemas alongside BookiE-specific authorization and execution. The protocol layer can be maintained by the Rust MCP SDK while token scopes, connection allowlists, approval, masking, audit and operation cancellation remain in BookiE. The design must keep those responsibilities at the governed seam.

## Decision

Use `rmcp` for the protocol, transport and tool-schema layer behind the governed seam. Keep everything that carries a safety decision:

- `McpBridge`, token scopes, connection allowlists, approval, masking and audit stay in BookiE's code and stay the only way a tool reaches a database ([ADR 0016](0016-governed-consumer-seam.md)). A tool handler holds a `PolicyGuard` connection and nothing else.
- Token and `Origin` checks and loopback-only binding move into `tower` middleware in front of the SDK's streamable-HTTP service. BookiE's token is still presented the way existing clients present it, so existing setups keep working.
- Tool input types become Rust structs with generated schemas; the hand-written `tool_schema` JSON goes away.

The target adapter must preserve authentication, authorization, disconnect cancellation and shutdown behavior. The SDK must not gain a path around BookiE policy or audit. The default server currently remains the hand-written implementation; the SDK adapter is an optional prototype. This decision defines the target and does not claim migration completion.

## Rationale

The protocol layer is maintained by the project that defines the protocol, and tracks the specification. BookiE retains application-specific authorization and audit code. The SDK's schema generation removes a class of drift between a tool's description and its real arguments.

## Consequences

Accepted:

- The SDK dependency tree requires license and advisory review under `deny.toml` and the supply-chain checks.
- Major SDK upgrades require explicit version review and validation.
- BookiE's token is not an HTTP `Authorization` bearer in all cases today; the middleware must preserve current behaviour rather than silently switching clients to OAuth.

Gained:

- Protocol compliance and future MCP features (resources, prompts, elicitation) come from the SDK.
- Schemas are generated from the same types the handlers read.

## Alternatives considered

- **Keep the hand-written server.** Works today; protocol features and specification changes require local implementation and validation.
- **Run an MCP server as a separate process.** Rejected for the same reason as [ADR 0001](0001-no-plugin-system.md): another trust boundary around credentials and sockets.
- **Adopt the SDK's OAuth support now.** Out of scope; it can follow once the transport is in place.
