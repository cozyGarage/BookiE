# Historical mutation first-run notes (2026-08-22)


Archived from [testing.md](../testing.md) on 2026-10-10. Current mutation
commands stay on the testing page; this records the first-run findings only.

Many surviving mutants are *equivalent* — a different program with identical
behaviour — and can never be caught. In `sql_lex::skip_span`, replacing
`offset + 1` with `offset - 1` shortens a comment span by one character, but
the scanner then reads that character as ordinary text and reaches the same
result, so no test can tell the difference.

The first run on 2026-08-22 tested 87 mutants across `sql_lex.rs` and
`sql_literal.rs`: 65 caught, 12 missed, 9 timed out. Two of the twelve were
real gaps rather than equivalents:

- Nothing tested an underscore in a PostgreSQL dollar-quote tag, though the
  tag validator explicitly allows one. `$my_tag$` had no coverage.
- `extract_named_parameters` advances by whatever `skip_span` returns without
  checking it is non-zero, while `statement_spans` filters for exactly that.
  A zero-length span would hang the parameter scanner. No input produces one
  today, so this is a missing guard rather than a live defect — but the
  asymmetry between two callers of the same function is the kind of thing
  that becomes a defect later.

A timeout is a finding too: it usually means the mutant produced an infinite
loop, which tells you a loop depends on a value nothing bounds.
