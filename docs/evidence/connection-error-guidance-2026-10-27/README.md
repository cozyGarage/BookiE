# Connection error guidance, 2026-10-27

Issue #1732, lane UX. Source base: `887678414`.

The connection dialog formatted transport errors directly. Initial table and view reads also formatted driver errors with internal operation prefixes. Those paths bypassed the classified messages already used when opening saved connections.

The regression `ui::connect_dialog::error_tests::connecting_with_a_bad_certificate_keeps_the_certificate_recovery_guidance` opens a candidate through a deterministic driver that rejects an unknown certificate authority. It failed on the original code because the certificate recovery guidance was absent. It passes with the shared formatter. It makes no network request and does not use real credentials or certificates.

Test, Connect and initial metadata reads now reuse the existing transport and driver presentation. Existing translation strings and TLS safety advice remain authoritative. The regression proves candidate opening; installed Test-button checks cover success and failure, not an actual TLS handshake.

Full preflight passed: 2,208 unit tests, 291 sandbox tests and one Redis check. All 179 GTK widget tests and app Clippy passed. The 13 existing error-presentation tests also passed, including certificate safety and keyring recovery advice. A rebuilt installed app passed `test_connection_reports_success_in_the_dialog` and `test_connection_reports_failure_in_the_dialog` under isolated Xvfb. Forgejo acceptance is deferred during infrastructure migration.

Release-Note: Fixed: Connection setup shows consistent recovery guidance for connection and metadata errors.
