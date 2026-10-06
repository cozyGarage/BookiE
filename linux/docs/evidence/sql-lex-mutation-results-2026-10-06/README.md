# SQL lexer mutation results

`tablepro-core`'s `sql_lex.rs` generated 110 mutants. The full follow-up run
caught 93, timed out on 13, marked 1 unviable, and reported 3 survivors. Two
survivors exposed uncovered behavior: ClickHouse backtick escapes and
PostgreSQL dollar-tag characters after the first character. Added assertions
now kill both. The remaining survivor changes `length > 0` to `length >= 0`
after `skip_span` has already returned a positive length; it is equivalent for
this call path. The original broad run timed out on 13 mutants that can stop
scanner progress; the focused bounded recheck below now catches all 13.

The raw reports are retained in `*-outcomes.json`. The first targeted recheck
killed the ClickHouse mutation and still missed the PostgreSQL interior-tag
mutation. After adding the `$a-b$` case, the final targeted check killed that
mutation too. A 24-mutant scanner-only recheck exposed a nested-comment
fixture whose inner terminator accidentally matched an `index *= 2` mutation;
the fixture was shortened, and the later bounded recheck catches the mutation.
The current focused scanner unit suite passes 14 tests.

## Follow-up triage, October 6

The focused selector initially found a missed guard mutation in
`dollar_quote_length`: with the guard changed from `||` to `&&`, MySQL dollar
text was incorrectly treated as a dollar-quoted body. The permanent regression
`a_mysql_dollar_delimiter_is_not_a_quoted_span` kills that mutation.

The scanner progress contract now runs representative inputs in a child test
process with a two-second deadline. This converts a mutated infinite loop into
a bounded test failure while checking the exact statement spans for nested and
ordinary comments, quotes, bracket identifiers, dollar quotes, separators and
Unicode. A focused rerun of 24 mutations across the original timeout locations
and related guards reports 24 caught, 0 missed and 0 timed out. The 13 original
timeout-classified mutations are all included and caught. Full results are in
the [MySQL dollar-boundary packet](../sql-lex-mysql-dollar-boundary-results-2026-10-06/manifest.json).

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
