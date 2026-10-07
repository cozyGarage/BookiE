# Validation playbook

Evidence availability checked October 3: some dated `target/quality` links below are unavailable in this checkout. They remain historical references, not current passes. See [the evidence consistency review](archive/architecture-consistency-review-2026-10-03.md#evidence-audit) and its inventory for exact paths; retrieve exact-SHA artifacts or rerun before using missing raw proof for acceptance.

Use this page to select checks, hand work to another agent, and review its evidence.
The executable catalog is [test-layers.json](../scripts/test-layers.json).
[Testing](testing.md) describes fixtures; [the sprint plan](bookie-0.2-sprint.md)
owns B3 and B4–B6 completion. A green layer certifies only its named scope.

## Current desktop target

The [current continuation](bookie-0.2-sprint.md#current-continuation-plan-2026-10-03)
orders B3, B4, then Arch/Omarchy/Hyprland native Wayland UI acceptance.
GNOME desktop on Debian is the required installed/package phase after Arch;
current B3/B4 implementation does not require that VM. GNOME 50 in the layer catalog
names GTK/libadwaita development-library requirements, not a required desktop shell.
Keep existing automated Linux checks and record unavailable environments as pending.

## Start here

Run from `linux/`, using the repository's Rust toolchain. Agents configured with
RTK should prefix commands with `rtk proxy` so raw diagnostics are retained.

### LT-TRUNG pre-push gate

LT-TRUNG has 16 CPUs, 30 GiB RAM and local Docker. Use its shared `target/`
directory and run the available hosted-equivalent layers before pushing:

```bash
python3 scripts/run-test-layer.py full app-server values drivers tls \
  postgres-release keyring security-policy supply-chain widgets ui \
  workflow-lint harness packaging-contracts change-contracts
```

This runs the fast workspace gate, app-server and cross-driver value contracts,
Docker driver/TLS/PostgreSQL fixtures, keyring and dependency policy checks.
The runner saves a commit-pinned report and logs under `target/quality/`.
Fresh local results and remaining gaps are owned by
[the release audit](archive/release-audit-2026-10-03.md); prior October 1–2 counts and
cache measurements are in [validation history](archive/validation-history.md).

Run `quick` for a short edit loop. Run the pre-push gate after changes that
touch shared core/value paths or before handing a B3 slice to review. Keep the
hosted workflow as the recorded platform/packaging check.

```bash
python3 scripts/run-test-layer.py --list
python3 scripts/run-test-layer.py harness
python3 scripts/run-test-layer.py quick
python3 scripts/run-test-layer.py change-contracts
python3 scripts/run-test-layer.py security-policy supply-chain
python3 scripts/run-test-layer.py drivers tls postgres-release
python3 scripts/run-test-layer.py widgets app-server keyring ui
python3 scripts/run-test-layer.py values
```

The fast Build Linux job runs `change-contracts` before the full GTK/unit layer.
It compares the checked-out commit with the pull request base or prior push
commit, runs mapped exact regression tests for changed value-path files, and
retains a report and per-command logs under `target/quality/`. Locally, check
uncommitted work against `HEAD`, or compare a branch with its base explicitly:

```bash
python3 scripts/run-change-contract-tests.py
python3 scripts/run-change-contract-tests.py --base fork/linux
```

Changes under `crates/core/src/`, `crates/core/tests/`, `crates/app/src/`,
`crates/mcp/src/`, and database-driver `src/` paths select focused exact
regressions where mapped, otherwise the changed package's unit or integration
tests. Edit `scripts/change-test-map.json` when adding a focused value
regression. Driver integration-test files stay in the Docker `drivers` layer,
which executes those server fixtures. The runner fails if Cargo returns success
without a passing test summary or if an expected exact test does not appear
exactly once as passed, with a single summary, no ignored or measured tests, and matching counts.
The named-test runners share `scripts/rust_test_evidence.py`; full workspace
summaries allow ignored fixture tests because their owning layers execute them.

Choose the layers affected by the change; the examples are separate invocations,
not a requirement to repeat overlapping unit suites. The runner executes selected
layers sequentially. Independent steps and later layers still run after a failure;
the overall result remains failed. Nothing retries automatically.
Layers can compose existing tiers; each test keeps its original tier ownership.

Each invocation writes `target/quality/<UTC timestamp>-layers/report.json` and
one log per step. It records the commit, dirty status, tracked-diff digest, exact
arguments, elapsed time, exit code and status. A dirty-tree report is development
evidence, not reproducible release-candidate evidence: commit the tested changes
before handing them off. The runner does not archive untracked file contents.
Inspect a live log with `tail -f` in another terminal; do not pipe the test command
through a command that discards its exit status.

For the standalone security and workflow layers, match the hosted tool versions:

```bash
cargo install cargo-deny --locked --version 0.20.2
cargo install cargo-audit --locked --version 0.22.2
go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12
```

Put Go's bin directory on PATH and install ShellCheck for `workflow-lint`.
Advisory databases remain live even with pinned tools; a later advisory failure
is new evidence, not a reason to freeze or suppress the database.

Missing tools, nonzero exits, timeouts, cancellations and absent required test
summaries cannot produce a pass. Cargo layers require at least one executed test
and no failed summaries. Environment-specific ignored tests still belong to their
dedicated fixture layers. Value and isolated-test runners enforce stricter
per-test execution contracts. Process exit alone is used for non-test tools and
scripts that enforce their own evidence contract; it is not a coverage claim.
SIGKILL, power loss or a runner crash can leave `status: running`; treat that as
incomplete, never passed. Review every step status, not just the final log line.

## Layers and boundaries

| Layer | What it verifies | Prerequisites / boundary |
| --- | --- | --- |
| `harness` | Failure handling, workflow wiring, test runner and function-size regressions | Python 3; no Cargo build |
| `workflow-lint` | Action schemas, expressions and embedded shell | actionlint v1.7.12 and ShellCheck |
| `packaging-contracts` | Candidate archive and Debian package validator behavior | Git, tar, `dpkg-deb`; no actual install or upgrade |
| `change-contracts` | Focused regressions or package tests selected from changed core, app, MCP and driver source paths | Rust 1.98; changed driver integration tests remain in the Docker `drivers` layer |
| `quick` | Guards, formatting, non-GTK Clippy, units and sandbox | Rust 1.98 and native libraries |
| `full` | Default workspace checks including app logic | GNOME 50 development stack; no display automation or DuckDB |
| `sandbox` | Integration tests without external servers or a display | Local sockets/processes must be allowed |
| `drivers` | PostgreSQL, MySQL, MSSQL, ClickHouse, Redis, MongoDB, MCP BSON, policy sessions, PostgreSQL socket and SSH | Docker and OpenSSH; SQLite is in sandbox, DuckDB has its own Build job |
| `values` | Exact values across eight drivers, core, app and MCP | Docker, GTK build dependencies, optional DuckDB build; strict selected-test counts |
| `tls` | CA, hostname, encryption and plaintext refusal against real servers | Docker Compose and generated private fixture certificates |
| `ssh` | SSH agent authentication and OpenSSH sessions | Docker and OpenSSH client tools |
| `postgres-release` | Policy, TLS/SSH, lock, cancellation, rollback and reconnect | Docker Compose, keyring/D-Bus and generated materials |
| `security-policy` | Policy/MCP permissions, allowlists, bounds and audit behavior | Non-GTK dependencies; does not prove every server authorization mode |
| `supply-chain` | Advisory, license and dependency-source policy | `cargo-deny`, `cargo-audit`, network; repository exceptions remain explicit |
| `widgets` | Named GTK widget regressions in isolated processes | GNOME 50, Xvfb and private D-Bus |
| `app-server` | Registered PostgreSQL/MySQL/MongoDB parser, keyed-edit and metadata contracts | GNOME 50 build libraries and Docker; hosted GTK fast job |
| `keyring` | Registered Secret Service contracts | Private D-Bus, gnome-keyring and libsecret tools |
| `ui` | Real application actions and database postconditions via AT-SPI | GNOME 50, Xvfb, PyAT-SPI; X11 automation, not Wayland acceptance |

Build/package, mutation, coverage and acceptance retain their existing entrypoints:

| Scope | Entry point | Completion evidence |
| --- | --- | --- |
| Optional DuckDB | Build Linux / `duckdb`; commands in [testing](testing.md#optional-features-and-ignored-tests) | Driver tests plus app feature build |
| Mutation | Linux test quality; [local instructions](testing.md#mutation-testing) | Selected/generated count, caught/missed/timeout/unviable counts and outcomes JSON |
| Coverage | Linux test quality; [local instructions](testing.md#coverage) | LCOV, exclusions and summary; no score threshold implied |
| Flatpak | Flatpak Linux (packaging only) | Artifacts for both profiles; not regression acceptance |
| Arch / Debian candidate | [Release guide](archive/release-0.1.4.md) and packaging scripts | Immutable SHA, package checksum, validator output |
| GTK soak | GTK Safety Soak | Five independent retry-free attempts at one resolved SHA |
| Installed acceptance | [Manual feature checklist](manual-verification-0.2-features.md) and sprint gates | VM/desktop version, Wayland session, package checksum, steps, result and screenshots |

## Hosted checks

Build Linux owns required regression jobs and its final `Linux regression gate`.
Each migrated layer job uploads reports even after failure. Missing artifacts
fail visibly. Linux Security runs policy tests and supply-chain checks in
independent jobs, including when a GTK build fails. Linux CI contracts tests the
validation infrastructure without waiting for a Rust build. Linux test quality
owns mutation/coverage. Security and quality results are separate from the Build
regression gate; review all of them at the same SHA.

Actions are pinned to commits. Jobs use read-only repository permissions and
ordinary `pull_request`, not privileged execution of PR code. Test fixtures use
disposable data and synthetic credentials. Do not give untrusted PR code access
to production databases or an unattended personal desktop/self-hosted runner.

On 2026-09-27 the fork's default branch was `main`, while these changes lived on
`linux`. GitHub schedules require the workflow on the default branch; manual
dispatch availability also depends on workflow registration. See the official
[schedule and dispatch rules](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows).
Do not infer execution from a cron declaration. Push/PR runs provide current
evidence; confirm the workflow is registered before relying on dispatch:

```bash
gh workflow list --repo cozyGarage/BookiE
gh workflow run build-linux.yml --repo cozyGarage/BookiE --ref linux -f ref=<full-commit-sha>
gh run list --repo cozyGarage/BookiE --branch linux
gh run view <run-id> --repo cozyGarage/BookiE --log-failed
gh run download <run-id> --repo cozyGarage/BookiE --dir /tmp/bookie-run-<run-id>
```

Check `headSha` and the report's resolved commit. Branch names move. Registering
these workflows on the default branch and choosing branch-protection/ruleset
requirements remain repository-administration tasks; this change does not modify
either. Path-filtered workflows may not run for unrelated changes. Do not require
a permanently absent check without reviewing those path filters.

## Build reuse and safe parallel work

Keep `linux/target` on disk and reuse the existing lockfile/toolchain. Do not run
`cargo clean` to fix a test failure. Feature/profile/RUSTFLAGS changes can trigger
rebuilds; DuckDB's native build is expensive. Run narrow tests before wider layers.

The layer runner holds a nonblocking checkout lock. A second layer runner fails
before executing commands. Raw Cargo, legacy scripts and mutation tools do not
take that lock: coordinate them explicitly. Never run concurrent Cargo or mutation
jobs against a shared target directory. The current single-agent session uses this checkout and cache directly. If
parallel owners are introduced later, use separate worktrees/targets or take turns. Docker Compose fixtures also
share project names; serialize fixture runs on a shared Docker daemon even across
worktrees. Do not clean up another agent's containers or volumes.

## Agent task template

Copy this and fill in every field:

```text
Repository/worktree: <absolute path>; branch: linux; expected full SHA: <sha>
Goal: <one invariant or failure>; sprint scope: B3 / B4–B6 acceptance
Allowed edits: <paths>; other agent-owned paths: <paths>
Run layers: <catalog names>; narrow reproducer: <exact command>
Environment available: <Docker/GTK/VM/keyring/network/tool versions>
Shared target and Docker fixture reservation: <owner/time or isolated setup>
Fix authorization: <test only / reproduce then fix>; push authorization: <yes/no>
Return: initial failure, cause, patch, tests before/after, report paths, SHA,
        skipped/blocked/not-run scopes, unresolved risks and hosted run URLs.
Do not weaken assertions, ignore survivors, retry into green, raise size baselines,
or count a missing fixture as passed. Preserve unrelated changes.
```

An agent must read `CLAUDE.md`, inspect status and confirm the SHA before editing.
If the environment is blocked, return the failing prerequisite and evidence;
do not replace a real server test with a mock and claim equivalent coverage.
All reports must distinguish local results, hosted results and installed acceptance.

## Turn every finding into a regression

1. Record origin: user report, upstream issue/commit, mutation, fuzz case or CI run.
2. State the invariant and smallest input; record engine/version and expected type
   as well as value. Preserve a minimized sanitized fixture, never customer data.
3. Add the lowest-tier reproducer and demonstrate failure before changing code.
4. Fix the cause; cover valid, invalid and exact-boundary neighbors. Test each
   affected consumer: decode, binding, formatting, grid editing, export and re-import.
5. Add real-server or UI evidence where pure tests cannot prove the behavior.
   Assert database state and returned values, not merely absence of an error.
6. Register ignored GTK/keyring tests in `isolated-tests.json`; regenerate the
   ignored-test ledger. New tiers need a catalog entry, local runner and hosted gate.
7. Run focused mutation checks with a nonzero selected count and independent
   expected values. Document each equivalent survivor with a concrete argument;
   leave unresolved survivors/timeouts open.
8. Update scenario evidence and the owning sprint task. Keep the test with the fix.

Use [the scenario survey](archive/b3-test-scenario-survey.md) and
[value contracts](value-contracts.md) as the B3 case backlog. Current examples:

| Encountered scenario | Permanent regression home | Next edge to investigate |
| --- | --- | --- |
| Rounded wide integers/decimals | Shared value corpus, core XLSX, PostgreSQL numeric contracts | Consumer edits and remaining numeric representations |
| Time 24:00 became midnight | PostgreSQL temporal units and real-server contracts | Temporal arrays, infinities and intervals |
| BC/year 10000 SQL export failed | Core SQL literal unit and PostgreSQL date round-trip | Remaining driver-specific calendar limits and non-SQL consumer parity |
| Decoder and oracle shared the same bug | Array size literal and numeric variant assertions | Independent oracles for other driver decoders |
| Green CI omitted eight SSH tests | SSH runner, ignored-test inventory and workflow regression | Audit new ignored targets whenever added |
| Empty/failed mutation report looked complete | Mutation summary fixtures and workflow regression | Triage core's 88 survivors/16 timeouts; no blanket exclusions |
| Test process failed, hung or never ran | Layer-runner negative tests | Keep adding actual infrastructure failure modes |

Secure connection coverage is engine-specific. Track valid auth, rejected auth,
wrong CA/hostname, no plaintext fallback, cancellation during connect/read/write,
disconnect/reconnect, pool reuse, partial streams, malformed lengths, NULL/empty
distinctions, Unicode/binary boundaries and authorization on every consumer.
An unchecked scenario is a backlog item, not an implied capability or a pass.

## Release decision

B3 requires closed correctness findings and their regressions. B4–B6 additionally
require the planned installed-package, Wayland, upgrade/rollback and candidate
soak evidence. Docker supplies deterministic servers and Xvfb supplies automated
GTK interaction. Current installed acceptance uses Arch/Omarchy under native
Wayland, on the host or an Arch VM with Hyprland. GNOME/Debian VM qualification
is the required next phase after Arch. Xvfb and widget passes cannot certify
either native Wayland target.
No workflow here automatically publishes a release or waives an acceptance gate.

## Initial implementation evidence, 2026-09-27

Local reports below are working-tree evidence based on `1b771214d`, not hosted
results for the later commit. Paths are relative to `linux/target/quality/`.

| Report directory | Result |
| --- | --- |
| `20260926T233834145290Z-layers` | 28 Python runner/workflow tests, standalone function-size regression and actionlint/ShellCheck passed for six Linux workflows |
| `20260926T233421164175Z-layers` | Non-GTK preflight, four isolated GTK widget tests and seven Secret Service tests passed |
| `20260926T232651996488Z-layers` | Policy/MCP tests, cargo-deny 0.20.2 and cargo-audit 0.22.2 passed |
| `20260926T233820141351Z-layers` | Arch candidate validator passed; Debian validator explicitly blocked by missing `dpkg-deb`; aggregate failed |

The runner has 13 focused regression tests, including false-success output,
empty execution, missing tools, timeouts, cancellation, later-layer evidence and
checkout contention. The broader harness totals 28 unittest cases. Existing
server/TLS/release/UI suites were wired to retained reports; they were not all
rerun locally for this infrastructure change. Hosted workflow execution and
installed-package/Wayland acceptance remain separate evidence.

## SonarCloud triage

Confirm a framework-specific finding against the native platform before changing source.

### GTK CSS node selectors

SonarCloud reports eight “Unknown type selector” findings on
`data/resources/style.css:25-26` for `columnview`, `listview`, `row` and
`cell`.

These are GTK CSS node names in the focus-ring selector. GTK documents
`columnview`, `listview` and `row` as nodes in the `GtkColumnView` /
`GtkListView` tree. GTK's `GtkColumnViewCellWidget` source sets its CSS node
name to `cell`. The stylesheet targets those nodes to draw the grid-cell focus
ring; converting them to classes would stop matching the widget tree.

Disposition: confirmed framework-specific false positives from the web CSS
analyzer. Keep the GTK selectors unchanged. The SonarCloud findings still need
their status updated in the project dashboard; no source suppression was added.

- [GTK ColumnView CSS nodes](https://docs.gtk.org/gtk4/class.ColumnView.html)
- [GTK ListView CSS nodes](https://docs.gtk.org/gtk4/class.ListView.html)
- [GTK ColumnView cell CSS name in GTK source](https://github.com/GNOME/gtk/blob/main/gtk/gtkcolumnviewcellwidget.c#L282-L299)
