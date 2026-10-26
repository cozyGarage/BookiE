# Saved connection row GTK capture, October 26

This follow-up records the screenshot helper change at source commit `8263c42`. It replaces X11-only `xdotool` and ImageMagick capture with `GtkWidgetPaintable`, `GtkSnapshot` and the window's GSK renderer. The helper saves the same row widget directly, so GTK tests do not need an external window-search or screenshot command.

## Acceptance scenario

Given a saved connection with a long Unicode name and endpoint, when the GTK test lays out the row with 20 px text at 320 px, then the connection name receives at least 120 px and the favorite, actions and open controls remain allocated. The test also captures the row at 320 px and 560 px in light and dark schemes.

## Evidence

- `scripts/test-gtk-widgets.sh`: 171 tests passed, including the saved-row allocation test.
- The focused screenshot run passed under Xvfb with GTK 4.24.1 and libadwaita 1.10.0. All four images were inspected. The narrow image shows the long labels ellipsized while the three actions remain visible.
- `bash scripts/preflight.sh`: passed. Nextest ran 2,147 unit tests and 291 sandbox tests, plus the Redis cancellation test. Structural guards, Clippy and formatting passed.
- The test still emits the recorded GtkBox min/natural-height warning at constrained widths. The bare `AdwActionRow` control comparison is documented in the [earlier evidence record](../saved-connection-row-2026-10-23/README.md).

An earlier native Wayland attempt under Hyprland tiled the preview window to 1,896 px instead of the requested 320 px; the helper rejected the capture. A later source-built run at Linux `96a8cce2aa51a788b880db4ddf90a4c73d3f0b13` used an isolated XDG profile containing one synthetic PostgreSQL connection and no credentials. At the application's 600 x 900 px minimum window size, the long Unicode name and endpoint truncated with ellipses, and the favorite, actions and open controls remained visible. Sequential Tab presses moved the visible focus indicator from Actions to Open. The screenshots are native Wayland captures under Hyprland.

This run does not test the 320 px child allocation from the GTK widget test and does not resolve the GtkBox minimum/natural-height warning. The log records that GTK could not connect to this session's AT-SPI socket (`Connection refused`). It used the mise Python, where `pyatspi` was not importable; Orca was not installed. Screen-reader narration and accessible-name verification therefore remained outstanding for this run. Issue #1332 remained open for those checks and review of the measurement warning.

## AT-SPI rerun

On 2026-10-26, the existing saved-row accessibility scenario passed under the isolated X11/AT-SPI test harness. The binary was built from PR #1686 head `1a08568dcdc63207577dd3d24269a4b3d1a2de0a`, based on Linux `589c64f02a1c8fb8dd69d1304e3a4149bf75e338`. The `connection_row.rs` and `gtk_ux.py` files were identical to that Linux base.

- Command: with `TABLEPRO_GTK_BINARY` pointing to the built binary from that worktree, `PATH=/usr/bin:/bin:/usr/local/bin:$PATH TABLEPRO_GTK_SCENARIO=saved_connection_row_controls_have_keyboard_focus_order bash scripts/test-gtk-safety.sh`.
- Binary SHA-256: `df136150e42d68e8c94481ce2ee516c70c8892cc1d690ea342bd271ff2ac483e`.
- Result: the selected scenario passed in 3.051 seconds. AT-SPI found the Favorite, Actions and Open controls by their accessible names and click actions; Tab moved focus in that order; Return opened the saved connection.
- `/usr/bin/python3` imports the installed `pyatspi` module. The default mise Python does not.
- Orca is not installed, so spoken screen-reader narration remains unverified. The run emitted `A connection to the bus can't be made` during shutdown after the scenario passed and the script returned exit status 0.

This rerun verifies the AT-SPI tree and keyboard path under X11. It does not replace the outstanding native Wayland screen-reader review, reproduce the 320 px allocation under Hyprland, or resolve the GtkBox minimum/natural-height warning. Keep #1332 open for those items.

## Screenshots

| Theme | Narrow, 320 px | Normal, 560 px |
| --- | --- | --- |
| Light | [light-narrow.png](light-narrow.png) | [light-normal.png](light-normal.png) |
| Dark | [dark-narrow.png](dark-narrow.png) | [dark-normal.png](dark-normal.png) |

Native Wayland, dark theme:

| Window | Actions focused | Open focused |
|---|---|---|
| [600 x 900 px](native-wayland-window.png) | [Tab 1](native-wayland-actions-focus.png) | [Tab 2](native-wayland-open-focus.png) |

SHA-256:

- `dark-narrow.png`: `93ed7432012b057f035ec4a2bf31cf6a838c11321b650e0453b176399b8d3181`
- `dark-normal.png`: `f0b6ee332d618f0eadf39032c186a2affbadfa46a02e58455cb180af7cf352c4`
- `light-narrow.png`: `4ff0275b7dd411e253cb153acbf892f80d4ceaef73b28e20563d4f1cff5ebde4`
- `light-normal.png`: `95daa6a317608b2b4be5bb82fc5ffcd08c210b8b7094f494b7a1c7cb2b59031b`
- `native-wayland-actions-focus.png`: `66885216350ba7941a7842f6ab8c1409b87d81c9de0a8b51668b31c81733f5d7`
- `native-wayland-open-focus.png`: `1a4a8fb4596d60b8e04785c1e33c8ef1d6c207076c208ed71d3beb40b8b592b8`
- `native-wayland-window.png`: `bfcdcb7d98fc0d8dad371224408abeb6aae12a5d2d4ae961fc5180202068d73d`
