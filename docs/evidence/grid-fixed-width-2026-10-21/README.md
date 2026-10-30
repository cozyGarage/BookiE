# Fixed grid width regression, October 21

Issue: [#1266](https://github.com/cozyGarage/BookiE/issues/1266), UX. Base: `64ad4ebf0`.

## Behavior

A saved or user-adjusted column width stays fixed when the window grows. Columns without a saved width continue to expand. Restoring or setting a fixed width disables expansion for that column; its persistence callback uses the signal argument instead of retaining the column.

## Regression coverage

The GTK regressions cover restoring a browse width, resizing a browse column, and resizing a result-grid column without connection persistence. They assert that the fixed column keeps its allocated width while an unsized neighbor expands; persisted cases also verify the stored width. Native Wayland drag delivery is not covered by these cases.
