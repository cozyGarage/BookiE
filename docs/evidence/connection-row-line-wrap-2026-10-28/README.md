# Saved connection row wrapping, October 28

The shared saved-connection row limited its title and subtitle to one line. At narrow widths this truncated long names and triggered a GTK `GtkBox` minimum/natural-height warning. Setting both limits to two lines lets long names wrap, bounds row height, and keeps the complete text in the existing tooltip.

This evidence covers the layout portion of [#1332](https://github.com/cozyGarage/BookiE/issues/1332). It does not complete the issue's native Wayland and spoken screen-reader review.

## Acceptance scenario

On a 320 px row with 20 px text, a long Unicode connection name wraps across two lines; its endpoint remains within two lines; Favorite, Actions and Open stay allocated. A 560 px layout is also captured. Test data contains no credentials.

## Evidence

- Code commit: `d20beec7e`; merged Linux base: `2f85de7a9`.
- Toolkit: GTK 4.24.1, libadwaita 1.10.0, GtkSourceView 5.22.0.
- The focused test failed with one-line caps: the name did not wrap, and GTK reported minimum height 52 px versus natural height 50 px at constrained widths 316 px and 179 px.
- With two-line caps, the focused test passed and the height warning was not emitted. It asserts that the title uses more than one and no more than two rendered lines, the subtitle stays within two lines, and all three row actions remain allocated.
- `bash scripts/test-gtk-widgets.sh`: 191 tests passed.
- `bash scripts/preflight.sh`: 2,246 unit/bin tests and 292 sandbox tests passed; structural checks, formatting and non-GTK Clippy passed. The preflight reported 89 existing historical evidence/source-drift warnings.
- `cargo clippy --locked -p tablepro-app --lib -- -D warnings`: passed.

The screenshots were rendered by the GTK test under Xvfb, not captured on native Wayland.

| Theme | Narrow, 320 px | Normal, 560 px |
| --- | --- | --- |
| Light | [light-narrow.png](light-narrow.png) | [light-normal.png](light-normal.png) |
| Dark | [dark-narrow.png](dark-narrow.png) | [dark-normal.png](dark-normal.png) |

SHA-256:

- `dark-narrow.png`: `3737127d16a755dbc5961a650ab09a733c07e9e5d77b43cb341fbf01d8416d8f`
- `dark-normal.png`: `aa7b0e4dd06f2ecd7ea676beb856f4514f395ee481982f1580ac9953628da3c9`
- `light-narrow.png`: `41518eae8ccf05f84eb7c54a15ca914a2fc59e56a74df8971b4efde71544a5a9`
- `light-normal.png`: `6d5664b0cb6aba04c08f13e693a69916722b1f64ea1d6d9c3a7b7d5ecdd8a7b4`
