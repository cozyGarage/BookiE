# Editor buffer and input-context teardown, 2026-10-26

Related: [#1299](https://github.com/cozyGarage/BookiE/issues/1299) and [#1333](https://github.com/cozyGarage/BookiE/issues/1333). This packet covers local Xvfb/Cairo checks. Native Wayland acceptance remains open.

## Environment and mechanism

The application base is Linux `15ad48925`. The host has GTK 4.24.1, GtkSourceView 5.22.0, libadwaita 1.10.0, fcitx5-gtk 5.1.7 and Fcitx5 5.1.23. A fresh `Gtk.IMMulticontext` reports `fcitx5` when `GTK_IM_MODULE` is unset. The profiled process maps `/usr/lib/gtk-4.0/4.0.0/immodules/libim-fcitx5.so`.

The standalone probes run in a private D-Bus session without a Fcitx5 daemon. They create four SourceView widgets per cycle, assign each 1,100,000 bytes of synthetic SQL comments, remove their pages and drop the Python references. GObject weak-reference callbacks count finalized views and buffers. The lifetime probes disable accessibility with `GTK_A11Y=none`; they do not need an application, database or BookiE callbacks.

## Object lifetime checks

Each four-cycle probe creates 16 views and 16 buffers. The results are sampled after removal, garbage collection and a one-second idle window.

| Teardown | Input method | Finalized views | Finalized buffers | Initial RSS | Final RSS |
|---|---|---:|---:|---:|---:|
| Drop references | Fcitx5 | 0 | 0 | 108.6 MiB | 191.5 MiB |
| Drop references | Simple control | 16 | 16 | 108.5 MiB | 134.3 MiB |
| Detach buffer, then drop | Fcitx5 | 0 | 16 | 108.7 MiB | 135.5 MiB |
| Retire input context, then drop | Fcitx5 | 16 | 16 | 108.7 MiB | 142.8 MiB |

Detaching the buffer releases its large contents, but leaves the retained view. Switching a retiring view to the built-in simple context releases the previous input context and allows both objects to finalize. This is a teardown action only; the live editor keeps its configured input method.

The initial probes sleep during their final idle interval. A separate drop-only probe pumps the GTK main context throughout that interval; it still finalizes zero views and zero buffers with Fcitx5. The application lifetime regression also pumps the main context while waiting for release.

The application fix performs both steps on the primary and split editor views during component shutdown. It leaves the previous buffer contents intact for any legitimate owner. GTK documents that [setting the buffer unreferences the previous buffer](https://docs.gtk.org/gtk4/method.TextView.set_buffer.html). Query snapshots and the recently closed tab record are captured by the application before component shutdown.

## Heap allocation check

Heaptrack 1.5.0 recorded the original 18-cycle SourceView control with the default input method and with `GTK_IM_MODULE=simple`.

| Input method | Peak tracked heap | Allocations live at process exit |
|---|---:|---:|
| Default, selected Fcitx5 | 327.21 MB | 320.98 MB |
| Simple control | 56.60 MB | 35.27 MB |

These are decimal MB from heaptrack. Its live-at-exit category does not distinguish unreachable allocations from reachable process-lifetime objects. The largest retained stacks pass through `gtk_text_buffer_set_text` and GtkSourceView. Private accessibility activation failed in these two profiler runs, so their allocation totals are supporting diagnostics; the separate lifetime probes avoid accessibility and establish the object-retention result directly.

## Regression scope

The new application regression holds both editor views after dropping the component and requires the old large buffer to finalize. It failed before buffer detachment and passed after it. The existing editor lifetime test now uses the default input method for the application views. It failed with `editor view remains alive` after buffer detachment alone; retiring the input context addresses that remaining lifetime.

Final local checks passed: 2,197 unit/bin tests, 291 sandbox tests, 176 isolated GTK widget tests, app clippy and four installed-app scenarios covering save before close, discard without writing, closing a clean editor and restoring the editor after restart. The checks include the existing policy, credential-storage and transport regressions; the fix changes no security rule.

This does not establish that every input method, compositor or native session has the same retention behavior. It does not close the broader freeze or large-SQL issues.

## Application workload after the fix

The source-built CI-profile app completed 18 cycles of four approximately 1.1 MB SQL tabs, closing all four and reopening an editor through the welcome button's accessibility action. A final query succeeded, a 15-second idle sample followed, then the process exited normally.

| Checkpoint | RSS |
|---|---:|
| Initial editor | 171.6 MiB |
| After closing cycle 1 | 218.3 MiB |
| After closing cycle 6 | 232.6 MiB |
| After closing cycle 12 | 237.5 MiB |
| After closing cycle 18 | 243.5 MiB |
| After idle | 246.1 MiB |

The highest sampled RSS was 252.6 MiB. Idle CPU was 0.09 CPU seconds over 15 seconds, or 0.60% of one core. RSS still rises modestly after the first cycles; this run does not establish that all remaining memory growth is a leak or that its allocation source is known.

The earlier [18-cycle record at source `9d82a5ebe`](https://github.com/cozyGarage/BookiE/blob/d3566e3b2/docs/evidence/editor-close-current-linux-2026-10-26/README.md) ended at 448.9 MiB with the default input method and 248.0 MiB with the simple control. The new default-input-method result is close to that control. These application runs use different source revisions and different welcome-button activation mechanisms, so they are contextual measurements rather than an isolated benchmark of a single patch. The lifetime probes and application regressions establish the release mechanism separately.

An initial attempt used this machine's `xvfb-run` default of a 640×480 screen. Its recorded app frame was 640×480, below the pointer helper's minimum window search size of 800×600. This was a fixture mismatch, not evidence of an application layout defect. The successful run explicitly used a 1280×1024 screen and accessibility activation. It does not prove physical pointer reachability or establish the reported native Wayland freeze.

## Records and reproduction

- [Drop-only lifetime probe](lifetime-control.py), [Fcitx5 result](lifetime-fcitx5.json) and [simple result](lifetime-simple.json).
- [Buffer-detachment probe](lifetime-detached.py) and [result](lifetime-detached.json).
- [Input-context retirement probe](lifetime-retired-im.py) and [result](lifetime-retired-im.json).
- [Drop-only probe with an active idle loop](lifetime-mainloop.py) and [result](lifetime-mainloop.json).
- [Default heap report](heaptrack-default.txt), [simple heap report](heaptrack-simple.txt), [default resource samples](default.json) and [simple resource samples](simple.json).
- [Application repeated-close scenario](app-cycles.py).
- [Application resource samples](app-fixed/resource-idle-samples.json), [scenario outcome](app-fixed/repeated_editor_close_cycles-result.json) and [binary hash](app-fixed/binary.sha256).
- [First-attempt display mismatch](first-attempt-display.txt).
- [Validation summaries](validation.txt), [default-input-method failure before context retirement](default-lifetime-before.txt), [source base](source-base.txt) and [source file hashes](source-files.sha256).

Run a lifetime probe from the repository root:

```sh
GTK_IM_MODULE=fcitx5 GTK_A11Y=none GDK_BACKEND=x11 GSK_RENDERER=cairo \
  dbus-run-session -- xvfb-run -a python3 \
  docs/evidence/editor-buffer-release-2026-10-26/lifetime-retired-im.py
```
