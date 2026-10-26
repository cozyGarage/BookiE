# Editor lifetime acceptance, 2026-10-26

These checks use the CI-profile binary from experimental fix commit `ab238b4acdacbcec941d11c82d7460c518685f45`, not the Linux baseline at publication. The [source packet](https://github.com/cozyGarage/BookiE/blob/ab238b4acdacbcec941d11c82d7460c518685f45/docs/evidence/editor-buffer-release-2026-10-26/README.md) owns the teardown mechanism and earlier measurements. The Forgejo gate is tracked separately on PR #1694.

## Layout, keyboard and editing

Seven [ordinary-environment runs](layout-keyboard-acceptance.json) passed: narrow-window controls, editor keyboard actions, saved-connection keyboard order, two Ctrl+Tab navigation cases, multiline paste preservation and refusing paste beyond loaded rows. Four [scale-2 runs](scale2-acceptance.json) passed: editor and connection-row keyboard access, clean-editor closing and restoration after restart. The scale-2 fixture used a 2560×2048 Xvfb screen. Both suites exited successfully and used the same binary hash as the source packet's Vim-follow-up validation.

These are eleven scenario runs, with overlapping cases at different scales. They establish the asserted interaction and persistence outcomes. They do not establish visual quality, high contrast, every window size, native Wayland behavior or general performance bounds.

## Repeated Vim editor closes

The same optimized binary completed 18 cycles of four approximately 1.1 MB SQL editors with Vim mode enabled, then executed a final query and exited normally. A normal-mode `h` motion first moved the caret without inserting text, confirming that the fixture activated Vim behavior. All 72 large editor tabs closed successfully. The [result](vim-result.json) and [samples](vim-cycles.json) record the outcome.

RSS rose from 173.2 MiB initially to 234.7 MiB after the final idle interval. The highest sampled high-water mark was 239.3 MiB. Idle CPU was 0.12 seconds over 15 seconds, or 0.80% of one core. This is one Xvfb/Cairo workload with ordinary allocator settings, not a process-memory bound or native-session acceptance. Portal helpers reported unavailable display interfaces in the isolated bus; visual and portal behavior were not qualified.

### Reproduction

Run the [Vim fixture wrapper](vim-app-cycles.py), which seeds isolated preferences and calls the [18-cycle workload](app-cycles.py), from the repository root. Supply a binary built from the recorded experimental fix; running the current Linux binary tests different code until that fix merges. The fixture uses synthetic SQL and SQLite data only.

```bash
mkdir -p .analysis/vim-lifetime
ATSPI_DBUS_IMPLEMENTATION=dbus-daemon XDG_CURRENT_DESKTOP=GNOME \
GDK_BACKEND=x11 GSK_RENDERER=cairo GTK_A11Y=atspi GIO_USE_VFS=local \
TABLEPRO_GTK_ARTIFACT_DIR="$PWD/.analysis/vim-lifetime" \
dbus-run-session -- xvfb-run -a -s '-screen 0 1280x1024x24' \
python3 docs/evidence/editor-lifetime-acceptance-2026-10-26/vim-app-cycles.py \
/path/to/verified/tablepro
```

The wrapper asserts live Vim cursor motion before the workload. A preceding attempt failed because the editor did not support the requested accessibility focus method; the successful wrapper uses the existing X11 click helper. The scripts assert normal app exit and preserve resource samples even if a later assertion fails. A failed result is not a completed 18-cycle measurement.

## Input and persistence boundaries

Five additional [installed-app scenarios](input-boundary-acceptance.json) passed on the same optimized binary: an overlong paste leaves the existing SQL unchanged; an oversized restored draft remains saved without loading into the editor; bundle import and export record sanitized audit outcomes; and a connection bundle imports unchanged into a second profile and survives restart. The suite exited successfully. These cases verify their specific refusal, persistence and audit assertions, not comprehensive import security or every malformed input.
