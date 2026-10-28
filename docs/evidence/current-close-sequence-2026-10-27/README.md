# Current Linux editor close sequence, 2026-10-27

Related: [#1333](https://github.com/cozyGarage/BookiE/issues/1333).

## Run identity

The Linux source was `3ca4a17aef1e50b59b8d71407be47893a7a5caf4`. The optimized release executable SHA-256 was `1eb5b45f377afc0b948fd19e8caa543310b4dd5fe205e71963a64f16f0156f25`. Tests ran on Arch Linux with isolated D-Bus, Xvfb, Cairo rendering, AT-SPI and synthetic SQLite data.

## Results

The 24 selected close and active-cell-edit scenarios from [the October 27 run](../local-acceptance-memory-2026-10-27/close-sequence-results.txt) passed with retries disabled. The set covers Save, Discard and Cancel, closing inactive tabs and groups of tabs, disconnect, window close, and save-before-quit. Assertions checked file/database contents, tab removal and usable follow-up editor actions.

The previously intermittent `active_cell_edit_saves_before_window_quit` scenario also passed in five fresh isolated sessions, without retries, in 2.9–3.9 seconds each. Each run saved the edited database value and observed the application frame close.

## Limits

This confirms the selected headless workflows on the recorded executable. It does not explain the earlier full-run timeout or reproduce the reported native Wayland freeze. The failure history remains valid; keep #1333 open for native-session recheck and root-cause diagnosis. No product code changed.
