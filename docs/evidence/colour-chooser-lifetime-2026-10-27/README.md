# Colour chooser lifetime, 2026-10-27

Issue #1729, lane UX. Dropping a Group and tags colour chooser leaves its manually parented popover alive. An isolated GTK weak-reference regression fails on the original implementation and passes when the regular Button is replaced by a MenuButton owning the popover through set_popover.

The regression also selects Blue and checks the selected colour index before dropping the chooser. No database, keyring or user file is accessed. Focused checks use isolated Xvfb, fatal GTK criticals and a simple input method. Full ordinary widget validation, app Clippy and preflight are pending. Forgejo acceptance is deferred during infrastructure migration.

Release-Note: Fixed: Closing connection organisation releases the colour chooser popover.
