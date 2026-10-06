# SQL lexer mutation results

`tablepro-core`'s `sql_lex.rs` generated 110 mutants. The full follow-up run
caught 93, timed out on 13, marked 1 unviable, and reported 3 survivors. Two
survivors exposed uncovered behavior: ClickHouse backtick escapes and
PostgreSQL dollar-tag characters after the first character. Added assertions
now kill both. The remaining survivor changes `length > 0` to `length >= 0`
after `skip_span` has already returned a positive length; it is equivalent for
this call path.

The raw reports are retained in `*-outcomes.json`. The first targeted recheck
killed the ClickHouse mutation and still missed the PostgreSQL interior-tag
mutation. After adding the `$a-b$` case, the final targeted check killed that
mutation too. A 24-mutant scanner-only recheck then exposed a nested-comment
fixture whose inner terminator accidentally matched an `index *= 2` mutation.
The fixture now uses a shorter inner comment; the final check catches that
mutation.

The 13 original timeouts split into five zero-length span mutants and eight
cursor arithmetic mutants. All five zero-length mutants came from consumers
trusting `skip_span` to return a positive length. The named-parameter rewriter
and SQL diagnostics scanner now ignore zero-length spans, matching
`statement_spans`. The focused five-mutant recheck caught all five with no
timeouts. The seven remaining timeout outcomes are cursor arithmetic mutants
that move a scanner index backwards or fail to advance; they remain killed by
the mutation runner timeout rather than counting as test failures.

The focused unit selector passed 12 tests. The follow-up run after the guards
passed all 535 core library tests and caught all five zero-length mutants.
See the [zero-progress guard evidence](../sql-lex-zero-progress-guards-results-2026-10-06/manifest.json).

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
