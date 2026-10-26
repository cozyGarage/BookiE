# Editor toolbar width regression

This record compares the reported toolbar minimum-width warning with the current Linux tree. It covers GTK allocation under Xvfb, not native Wayland pointer interaction or installed-package acceptance.

## Reproduction

- Historical source: `20ee44f3e1e3fdfc5b2141313323c3279fc5f0e0`.
- Window width: 1,280 px.
- Simulated visible sidebar: 370 px.
- Allocated editor width: 910 px.
- Visible state: running spinner, Cancel button and a streamed-row status label.
- Result: the toolbar requested 967 px. GTK logged `AdwToolbarView exceeds AdwBreakpointBin width` and the width assertion failed.

The historical check used a temporary ignored GTK test with the same dimensions and visible controls. The temporary test was not added to the repository or its test inventory.

The temporary baseline test was run with:

```bash
G_DEBUG=fatal-criticals GDK_BACKEND=x11 GSK_RENDERER=cairo GTK_A11Y=none GIO_USE_VFS=local \
  dbus-run-session -- xvfb-run -a -s '-screen 0 1600x1200x24' \
  cargo test --locked -p tablepro-app --lib \
  ui::editor::dispatch_tests::reported_toolbar_minimum_fits_at_910px_with_sidebar_and_streaming_status \
  -- --ignored --exact --test-threads=1 --nocapture
```

## Current regression test

- Source: `0208ec6f5ea34ca9f5dd6dc74435f0e9a34233a7`, with the test added in `30cb7fdca475b323a521109a478a077efd3fd374`.
- Test: `ui::editor::dispatch_tests::editor_actions_fit_narrow_rows_with_running_and_retry_controls`.
- The test gives a 370 px sidebar and verifies actual editor allocations at 600, 630 and 910 px, including a 20 px toolbar text style. It checks that the `ToolbarView`, both toolbar bars, visible controls and transaction actions fit inside the editor.
- Result: passed under Xvfb with fatal GTK criticals enabled; no toolbar minimum-width warning was emitted.

Command used for the current source:

```bash
G_DEBUG=fatal-criticals GDK_BACKEND=x11 GSK_RENDERER=cairo GTK_A11Y=none GIO_USE_VFS=local \
  dbus-run-session -- xvfb-run -a -s '-screen 0 1600x1200x24' \
  cargo test --locked -p tablepro-app --lib \
  ui::editor::dispatch_tests::editor_actions_fit_narrow_rows_with_running_and_retry_controls \
  -- --ignored --exact --test-threads=1 --nocapture
```

## Limits

The test protects the measured layout contract. Separate coverage activates Run and Cancel in a narrow editor. This record does not establish native Wayland visuals, accessibility hit testing, larger system text scaling, or installed-package acceptance. Keep issue #1279 open for those checks.
