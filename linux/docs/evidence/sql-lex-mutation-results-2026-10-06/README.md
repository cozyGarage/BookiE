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
mutation too. The focused unit selector passed 12 tests.

The latest tested `sql_lex.rs` SHA-256 was
`22036d3d934ecac6206bfe335bd3819d765e2c7d19857c9a84d77a999c261f39`.

Commands, from `linux/`:

```bash
cargo test --package tablepro-core sql_lex::tests -- --nocapture
cargo mutants --package tablepro-core --file crates/core/src/sql_lex.rs --test-tool cargo --output target/quality/20261006-sql-lex-mutants-after-tests-c2a3ced7 --timeout 8 --in-place -- --lib
cargo mutants --package tablepro-core --file crates/core/src/sql_lex.rs --re 'sql_lex.rs:(21:89|97:67)' --test-tool cargo --output target/quality/20261006-sql-lex-survivor-recheck-c2a3ced7 --timeout 8 --in-place -- --lib
cargo mutants --package tablepro-core --file crates/core/src/sql_lex.rs --re 'sql_lex.rs:97:67' --test-tool cargo --output target/quality/20261006-sql-lex-final-mutant-check-c2a3ced7 --timeout 8 --in-place -- --lib
```
