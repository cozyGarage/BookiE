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

## Repeated oversized-file refusals

The fixed CI-profile binary also completed 18 refusals of an 8 MiB single-line SQL file in one process. Each refusal preserved the existing query, editor tab count and source file. Each batch of three ended with a successful query. The process exited normally. The [scenario](refusal-repeat.py), [per-refusal measurements](refusal-repeat/refusal-cycles.jsonl) and [process samples](refusal-repeat/samples.json) record the workload.

RSS reached 203.7, 236.8, 269.0, 279.0, 291.0 and 301.9 MiB after successive batches. It remained 301.8 MiB after 15 seconds idle, with 0.11 CPU seconds consumed. This does not establish a memory plateau. Repeated file-dialog use remains a separate resource investigation; the editor teardown fix does not claim to resolve it.

A same-binary run with the simple input method failed at an accessibility action before producing a comparable workload. Its [result](refusal-simple/repeated_overlong_file_refusal-result.json) is inconclusive, not evidence for an input-method cause. An earlier attempt could not activate the accessibility registry; subsequent runs explicitly selected `ATSPI_DBUS_IMPLEMENTATION=dbus-daemon`.

A separate [chooser-widget control](chooser-control.py) creates, maps, unparents and destroys 18 `Gtk.FileChooserWidget` instances without reading SQL or running BookiE. GObject weak callbacks observed all 18 instances finalize with both [Fcitx5](chooser-fcitx.json) and the [simple input method](chooser-simple.json). Final RSS was 125.5 MiB and 123.4 MiB respectively. This control does not exercise `Gtk.FileDialog`, portal lifetime, file selection, accessibility or the application's callback paths. It rules out retained chooser widgets in this narrow control, not those other mechanisms.

After correcting the chooser action race in harness source `b5e072fa818ea3f77fcfb7ec2c8a34a27f354b83`, the same binary completed the simple-input-method comparison: [acceptance result](refusal-simple-fixed/repeated_overlong_file_refusal-result.json), [per-refusal measurements](refusal-simple-fixed/refusal-cycles.jsonl) and [process samples](refusal-simple-fixed/samples.json). All 18 refusals preserved the query, tab count and file, and the process exited normally. Final idle RSS was 302.3 MiB, with 0.20 CPU seconds over 15 seconds. The earlier default-input-method workload ended at 301.8 MiB. These runs do not support changing the live input method as a fix for this remaining growth. They do not identify the retained allocations.

The comparison's private portal session logged an unavailable secret service, including a 120-second activation timeout. This limits timing and CPU comparison. It does not invalidate the successful content-preservation assertions or establish a product keyring failure.

A [native dialog cancellation control](dialog-control.py) separately opened and cancelled 18 `Gtk.FileDialog` instances with the simple input method. Each callback completed and all 18 dialog objects finalized. [Final RSS](dialog-simple.json) was 123.8 MiB. No SQL was read, and accessibility was disabled. This excludes dialog-object retention in this cancellation-only control; it does not exercise accepted selections or BookiE callbacks.

## Application allocation profile

Heaptrack 1.5.0 completed the same 18-refusal workload on the fixed CI-profile binary, using chooser harness source `64b3e7d1d`. The [result](heaptrack-refusals/result.json) passed and the application exited normally. The [wrapper](profile-refusals.py) adds the preload library only to the application environment returned by the fixture, preserving direct application PID tracking. The [process samples](heaptrack-refusals/samples.json) and [allocation report](heaptrack-refusals/allocations.txt) are retained here.

The 171.30-second recording reports 126,463,983 allocation calls, a 78.91 MB peak tracked heap and 445.27 MB peak RSS including profiler overhead. Its live-at-exit category is 58.70 MB; it includes process-lifetime toolkit and renderer allocations and is not proof of unreachable memory. The largest allocation-call category is `g_malloc`: 106,648,174 calls, with 4.51 MB peak consumption. The leading reported subpaths pass through GLib variant construction and GIO D-Bus message decoding. The report aggregates the full run; it does not attribute every allocation to accessibility or demonstrate its growth over individual cycles.

This separates tracked live heap from RSS and points to substantial allocation churn. Allocator retention, mappings, native memory and instrumentation overhead remain possible contributors to the gap. No product allocator setting or input-method workaround follows from this result alone. The private portal session again logged an unavailable secret service; timing comparisons remain limited.

The multi-gigabyte raw recording was removed after successful interpretation and compressed-file integrity validation. The 25,716,072-byte compressed trace remains in the ignored local analysis directory. The public evidence contains reports and reproducible scripts, rather than the binary trace.

## Allocator arena control

The same fixed binary and corrected chooser harness completed 18 refusals with `GLIBC_TUNABLES=glibc.malloc.arena_max=2` and `GTK_IM_MODULE=simple`. The [result](arena-control/result.json) passed and the app exited normally. [Samples](arena-control/samples.json) show RSS rising from 168.8 to 208.0 MiB, compared with 173.4 to 302.3 MiB in the earlier simple-input-method run. Sampled application CPU was 50.23 seconds versus 64.15 seconds, and the initial-to-final interval was 159.57 versus 167.15 seconds. These are single runs, not a latency benchmark. Both include private portal secret-service activation failures.

The setting was inherited by the private D-Bus fixture and its helpers as well as the app. This limits causal attribution. The [GNU libc manual](https://sourceware.org/glibc/manual/latest/html_node/Memory-Allocation-Tunables.html) documents the arena limit; the result supports allocator arena retention as a contributor in this workload. It does not prove that all retained memory is releasable or that a process-wide arena limit is suitable for database workloads. No application, desktop or runner allocator setting changed. A child-process-only repetition is needed before considering any wider tuning.
