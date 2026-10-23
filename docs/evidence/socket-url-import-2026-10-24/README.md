# Installed socket URL import acceptance

Issue: [#579](https://github.com/cozyGarage/BookiE/issues/579).

The installed scenario `imported_mysql_socket_urls_survive_restart` pins existing behavior. It imports both MySQL and MariaDB URLs, checks the MySQL driver and exact socket path in persisted connection records, and verifies that synthetic passwords reach Secret Service rather than connection JSON.

It restarts the application, checks both unchanged records and their reopened connection forms, then exports an encrypted bundle. Before importing into an empty second profile, it clears only the synthetic records from the private test keyring. The import must restore both exact connection records and both passwords. This prevents pre-existing credentials from making the restoration check pass.

## Result

The complete scenario passed without retries in 17.112 seconds under private Xvfb, D-Bus and Secret Service. It uses the unchanged development executable qualified with the [connection choice fix](../connection-choice-controls-2026-10-23/README.md), SHA-256 `befb19deb593cf7e838212637930e17b21eaea6ba51a3f3cb6467228aa0f7a07`. Test source branches from Linux `575150120`.

The first probe incorrectly expected a connected window after an import-only restart. Correcting the expectation to the welcome screen resolved that test setup error. No product defect or product behavior change is claimed.

## Remaining acceptance

This scenario does not connect to a real MySQL service through either imported socket. Native desktop acceptance and the Forgejo gate remain separate. The issue stays open until that acceptance is satisfied.

Local preflight also passed: 2,073 unit tests, 2,086 sandbox tests and the separate Redis cancellation check. The scenario itself ran on the unchanged executable identified above; no GTK widget production code changed.
