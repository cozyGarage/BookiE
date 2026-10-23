# Grid search focus, October 22

Status: implementation under investigation on Linux `e483fe07b`; final local validation has not passed. Hosted checks, maintainer Forgejo and native/package acceptance remain pending. Refs [#1319](https://github.com/cozyGarage/BookiE/issues/1319).

## Cause and change

On Linux `8450cb4d4`, accepting Jump to Column leaves keyboard focus on the Session button in the isolated app fixture. Asking the grid for focus while its search dialog is still modal does not establish row navigation. Selectable cell labels can also consume Home/End as text commands.

Both grid searches now use one close-time handoff. It retains weak widget references, snapshots focus after close, and refuses restoration when focus changes or the grid is unmapped. Selection and scrolling happen after the modal has released focus. A weak selected column is revalidated against the live grid; row selection keeps its bounds check. Read-only cells use native focusable wrappers, with their current row/column identity bound and cleared alongside the label, so keyboard focus does not drift into a neighboring label.

This follows the existing [ownership decision](../../decisions/0008-connection-and-session-ownership.md). No driver, value, policy or dependency contract changed. The current behavior is documented in the [user guide](../../user-guide.md#run-sql).

## Regression coverage

- The final 5,000-row scenario fails against the unmodified private Linux executable with focus on Session. It requires a focused row or cell, positive viewport bounds and a showing state, rather than merely finding an accessible label.
- The corrected path checks Ctrl+End/Home, acceptance and Escape for both Jump to Column and Find in Loaded Rows, including the exact selected row 2,500.
- The existing 300-column scenario retains its alignment assertions for columns 150, 300 and 1 and now also requires keyboard focus on the chosen cell. The stricter check rejected a candidate that kept the target visible but moved focus two columns left. Successful scroll positions can be captured with the standard artifact directory on X11.
- Two registered isolated widget tests cover animations enabled/disabled, hidden grids, a newer close-time focus choice, and weak grid lifetime. The deferred reveal callback is also required to stay uncalled for stale focus or hidden grids.

Exploratory candidates that restored focus before applying the target were rejected by the exact-row and wide-column regressions. Their local failure captures are retained separately from the final validation. Existing tests and assertions were preserved.

## Reproduction

From a source build, set `TABLEPRO_GTK_BINARY` to a private copy of the executable and run:

```bash
TABLEPRO_GTK_SCENARIO=grid_search_dialogs_return_keyboard_navigation_to_large_results bash scripts/test-gtk-safety.sh
TABLEPRO_GTK_SCENARIO=wide_result_jumps_by_keyboard_and_keeps_headers_over_their_cells bash scripts/test-gtk-safety.sh
```

Use a Python interpreter with `pyatspi`; the local matrix used `/usr/bin/python3` through `PATH=/usr/bin:/bin`. `ADW_DEBUG_COLOR_SCHEME=prefer-light` and `prefer-dark` select the two local appearance cases. `TABLEPRO_GTK_ARTIFACT_DIR` retains results, stderr and successful scroll-position screenshots when a supported X11 capture tool is available.

## Scope

The local matrix uses Arch Linux, private D-Bus/Xvfb, Cairo rendering and isolated SQLite fixtures. It is a source-built debug executable, not an installed release package or native Wayland run. No performance, whole-application stability, screen-reader qualification or Forgejo result is claimed. No existing database container was stopped or changed. Review is a separate self-review, not independent review.
