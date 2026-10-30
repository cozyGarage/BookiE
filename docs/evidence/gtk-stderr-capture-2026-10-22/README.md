# GTK test child stderr capture

Issue [#1279](https://github.com/cozyGarage/BookiE/issues/1279) requires test-child stderr to remain drained or file-backed.

The harness redirects child stderr to a temporary file and reads it after the child stops. This preserves diagnostics without allowing a full pipe to block startup or a scenario.

Regression coverage exercises a child that writes a large diagnostic stream before readiness. Cases cover successful startup, nonzero exit, spawn failure and missing-window cleanup. This change affects test harness behavior, not application logging or database behavior.
