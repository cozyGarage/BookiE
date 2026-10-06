# SQL lexer mutation results

`tablepro-core`'s `sql_lex.rs` generated 110 mutants. The full follow-up run
caught 93, timed out on 13, marked 1 unviable, and reported 3 survivors. Two
survivors exposed uncovered behavior: ClickHouse backtick escapes and
PostgreSQL dollar-tag characters after the first character. Added assertions
now kill both. The remaining survivor changes `length > 0` to `length >= 0`
after `skip_span` has already returned a positive length; it is equivalent for
this call path. The timeouts came from mutants that can stop scanner progress;
they remain open for B3-P6 triage.

The raw reports are retained in `*-outcomes.json`. The first targeted recheck
killed the ClickHouse mutation and still missed the PostgreSQL interior-tag
mutation. After adding the `$a-b$` case, the final targeted check killed that
mutation too. A 24-mutant scanner-only recheck then exposed a nested-comment
fixture whose inner terminator accidentally matched an `index *= 2` mutation.
The fixture now uses a shorter inner comment; the final check catches that
mutation, while the backward-index mutation still times out as expected.
The focused unit selector passed 12 tests.

## Follow-up triage, October 6

An exact-span selector was run against the five function-body replacements that
returned zero/empty spans and had timed out in the full suite. It caught those
mutants without timing out. That focused run also missed a guard mutation in
`dollar_quote_length`: with the guard changed from `||` to `&&`, MySQL dollar
text was incorrectly treated as a dollar-quoted body. The permanent regression
`a_mysql_dollar_delimiter_is_not_a_quoted_span` now kills that mutation. Seven
index-progress mutations still timed out in the narrowed recheck; one other
mutation from the original full run was caught in follow-up. Keep scanner
mutation triage open until those timeouts are individually classified. See the
[MySQL dollar-boundary packet](../sql-lex-mysql-dollar-boundary-results-2026-10-06/manifest.json).

The latest tested `sql_lex.rs` SHA-256 was
`90de78b764df0fddc7369bef7b95c08b6838fd6b44b0b4750d4c3aacd325f148`.

Commands, from `linux/`:

```bash
cargo test --package tablepro-core sql_lex::tests -- --nocapture
cargo mutants --package tablepro-core --file crates/core/src/sql_lex.rs --test-tool cargo --output target/quality/20261006-sql-lex-mutants-after-tests-c2a3ced7 --timeout 8 --in-place -- --lib
cargo mutants --package tablepro-core --file crates/core/src/sql_lex.rs --re 'sql_lex.rs:(21:89|97:67)' --test-tool cargo --output target/quality/20261006-sql-lex-survivor-recheck-c2a3ced7 --timeout 8 --in-place -- --lib
cargo mutants --package tablepro-core --file crates/core/src/sql_lex.rs --re 'sql_lex.rs:97:67' --test-tool cargo --output target/quality/20261006-sql-lex-final-mutant-check-c2a3ced7 --timeout 8 --in-place -- --lib
cargo mutants --package tablepro-core --file crates/core/src/sql_lex.rs --re 'sql_lex.rs:(2:5|29:5|48:5|62:5|69:23|78:24|85:5|116:19|121:19|125:15)' --test-tool cargo --output target/quality/20261006-sql-lex-timeout-recheck-c2a3ced7 --timeout 8 --in-place -- --lib sql_lex::tests
cargo mutants --package tablepro-core --file crates/core/src/sql_lex.rs --re 'sql_lex.rs:69:23' --test-tool cargo --output target/quality/20261006-sql-lex-nested-comment-recheck-c2a3ced7 --timeout 8 --in-place -- --lib sql_lex::tests
```
