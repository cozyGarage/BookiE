# Saved connection row GTK capture, October 26

This follow-up records the screenshot helper change at source commit `8263c42`. It replaces X11-only `xdotool` and ImageMagick capture with `GtkWidgetPaintable`, `GtkSnapshot` and the window's GSK renderer. The helper saves the same row widget directly, so GTK tests do not need an external window-search or screenshot command.

## Acceptance scenario

Given a saved connection with a long Unicode name and endpoint, when the GTK test lays out the row with 20 px text at 320 px, then the connection name receives at least 120 px and the favorite, actions and open controls remain allocated. The test also captures the row at 320 px and 560 px in light and dark schemes.

## Evidence

- `scripts/test-gtk-widgets.sh`: 171 tests passed, including the saved-row allocation test.
- The focused screenshot run passed under Xvfb with GTK 4.24.1 and libadwaita 1.10.0. All four images were inspected. The narrow image shows the long labels ellipsized while the three actions remain visible.
- `bash scripts/preflight.sh`: passed. Nextest ran 2,147 unit tests and 291 sandbox tests, plus the Redis cancellation test. Structural guards, Clippy and formatting passed.
- The test still emits the recorded GtkBox min/natural-height warning at constrained widths. The bare `AdwActionRow` control comparison is documented in the [earlier evidence record](../saved-connection-row-2026-10-23/README.md).

The native Wayland attempt ran under Hyprland. The compositor tiled the preview window to 1,896 px and did not honor the requested 320 px top-level size; the helper rejected the capture rather than saving a misleading narrow screenshot. The captured GTK warnings were 52/50 px and 69/65 px minimum/natural heights. This is not native visual acceptance. Orca and pyatspi were unavailable, so screen-reader narration was not run. Issue #1332 remains open for native visual and screen-reader acceptance.

## Screenshots

| Theme | Narrow, 320 px | Normal, 560 px |
| --- | --- | --- |
| Light | [light-narrow.png](light-narrow.png) | [light-normal.png](light-normal.png) |
| Dark | [dark-narrow.png](dark-narrow.png) | [dark-normal.png](dark-normal.png) |

SHA-256:

- `dark-narrow.png`: `93ed7432012b057f035ec4a2bf31cf6a838c11321b650e0453b176399b8d3181`
- `dark-normal.png`: `f0b6ee332d618f0eadf39032c186a2affbadfa46a02e58455cb180af7cf352c4`
- `light-narrow.png`: `4ff0275b7dd411e253cb153acbf892f80d4ceaef73b28e20563d4f1cff5ebde4`
- `light-normal.png`: `95daa6a317608b2b4be5bb82fc5ffcd08c210b8b7094f494b7a1c7cb2b59031b`
