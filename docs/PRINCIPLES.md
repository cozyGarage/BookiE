# Engineering principles

The top of the altitude order. These outrank local preference. An explicit edit to this file is the only way to change them.

- **Security first.** Validate input at system boundaries. Deny unsafe operations by default. Every consumer database handle is a `PolicyGuard`.
- **No account gate.** Every shipped feature works without an account, license, subscription, paid tier or remote entitlement check.
- **Linux-native client.** GTK4, libadwaita, Relm4. No web view and no source tree for another operating system.
- **Static drivers.** Engine support is a workspace crate registered at compile time, not a plugin ABI.
- **One home per fact.** Write a rule or a number once. Link to it. If a restatement must exist, declare it.
- **Decisions before construction.** Accepted ADRs bind work in their scope. Supersede a record; do not edit it into a different decision.
- **Secrets stay in Secret Service.** Do not write passwords, tokens or SSH secrets into JSON, command lines, traces, errors or audit fields.
- **Fix the cause.** Reproduce or trace a defect before changing code. Every testable behaviour change needs a regression test in the lowest tier that can show it.
- **Small, reviewable changes.** One concern per change. Do not mix unrelated cleanup into a fix.
- **Forgejo is the merge gate.** GitHub jobs may advise. Do not silence a Forgejo merge tier to land standards work.

Operational rules, lanes and commands live in [AGENTS.md](../AGENTS.md).
