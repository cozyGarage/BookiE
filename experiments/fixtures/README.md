# Shared large-cell regression inputs

`large-cells.json` describes synthetic values as `prefix + unit.repeat(repeats) + suffix`. Both prototype test suites expand the same cases without storing large generated files.

The cases cover multibyte text, raw JSON spacing, control characters and a binary value with a NUL and invalid UTF-8 tail. Byte values use hexadecimal text with a `\x` prefix, matching the experimental value envelope.

The tests require bounded visible previews and preservation of the complete source value. GPUI also checks conversion through the shared core value types. Display suffixes may differ between the two renderers. These are component regression inputs, not an installed-application benchmark or a bound on IPC payload size.
