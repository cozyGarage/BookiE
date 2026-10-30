# Test design rules

Rules that earlier audits paid for. Each one names the failure it prevents. The test layers and gates are in the [validation playbook](validation-playbook.md) and [testing](testing.md).

## Ownership and execution

- **An ignored or isolated test has an owner.** Registering it in the inventory is not enough: a job must run it. When adding or auditing an `#[ignore]` test, name the script and CI job that executes it, and keep a workflow regression that fails if the runner is disconnected.
- **A green workflow proves only its own scope.** A packaging run that builds both profiles and runs no regression suite says nothing about behaviour. Say which suite ran when quoting a result.

## Runners and evidence parsers

- **A runner test must reject wrong input.** Cover a wrong test name, repeated output, an ignored count, a nonzero exit and a registration that never ran. An isolated runner once accepted the first registered test's output for every registration ([regression audit, 2026-09-29](archive/regression-audit-2026-09-29.md), R4).
- **Parse stdout and stderr separately.** Retain both; parse only the test's stdout. Driver and graphics diagnostics interleave with unfinished lines and break a strict parser (same audit, R11).
- **Never mark an unavailable path as passed.** If raw evidence is missing, retrieve the exact-SHA artifact or rerun the gate. A hash cannot recover missing output, and a passing diagnostic probe is not a passing regression.

## Oracles and assertions

- **A parser is not its own oracle.** Testing a lexer or decoder only against itself hides shared mistakes. Take expected values from the engine or another independent source ([ADR 0007](decisions/0007-type-and-value-preservation.md), [oracle review](oracle-review.md)).
- **Assert concrete boundaries and both branches.** Mutation checks exposed symbolic constants in assertions and tests that exercised only one branch. Use the literal boundary value, and test the refusal as well as the accepted case.
- **Unviable mutants are not catches.** A mutant that does not compile is neither a coverage success nor an equivalent mutant; report it separately, and report a missing mutation report as unavailable, not as zeros.
- **A negative test proves a trusted control first.** For TLS and similar failures, show the same setup succeeds, then assert the specific error. `is_err()` lets an unrelated failure pass.
- **Test the race, not only the error.** For cancellation and similar features, cover caller drop, a deadline during dispatch, a delayed stop after completion, a transport failure, pool reuse and shutdown, and assert the server's behaviour as well as the returned error ([ADR 0005](decisions/0005-server-side-cancellation.md)).

## Escaping and decoding

- **Every dialect's literal rendering needs a hostile-value test.** A rendered INSERT that escaped only quotes let a value ending in a backslash close the literal early on MySQL and ClickHouse, and MySQL evaluated the rest as SQL. Test backslashes, quotes and each dialect's escape modes against a real server.
- **Fix a loss at its source, not by accepting the symptom.** Every SQLite column with no declared type once decoded as NULL, so expressions such as `count(*)` showed an empty cell (same record). Do not make a test pass by treating the empty result as expected.

## Changing a test or guard

- **Do not delete a test that pins a wrong expectation.** Amend it, and say in the commit why the expectation changed. Write the failing regression first. Crate-boundary checks must keep permitted dev edges (driver to policy) distinct from forbidden GUI edges.
- **Weakening a contract guardrail needs the contract owner's agreement** ([AGENTS.md](../AGENTS.md)).
