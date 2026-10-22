# Editor callback lifetime, 2026-10-22

Related: [#1333](https://github.com/cozyGarage/BookiE/issues/1333). Earlier allocation measurements are in [the lifetime investigation](../editor-lifetime-investigation-2026-10-22/README.md).

## Controlled reproduction

Plain GTK TextView and GtkSourceView controls retained native references under the default input method in the isolated X11 environment. Button and Entry controls released normally. Clearing window focus did not change this. With the process-local simple input method, all controls released. Selecting the simple input method on each TextView through its widget property also made those controls release. The environment used GTK 4.24.1, GtkSourceView 5.22.0 and libadwaita 1.10.0.

The regression selects the simple input method only on its fixture views and first requires a plain native SourceView to release. Under that controlled setup, the unfixed editor root released but its source view remained alive after dropping the component. After the callback correction, the root, view and buffer all release. Both runs used the same test, with no process-global input-method override. This isolates application retention from the separate native input-method behaviour.

## Cause and candidate

Buffer callbacks captured their owning source view, while the view owned the buffer. The completion refresh closure also captured the view strongly. Find/Replace callbacks retained handles containing their own search context, and the search entry callback captured its own entry.

Cursor and changed handlers now use their buffer argument. Completion refresh keeps a weak view reference. Find/Replace signal handles keep weak references and upgrade them only for the duration of an action. The component retains ownership while it is alive. Production input-method selection is unchanged; there is no forced widget disposal.

The narrow failing-before regression reported `editor view remains alive`; the corrected test passed. Full preflight, application all-target Clippy and all 105 registered GTK widget tests passed. The optimized allocation comparison is pending. No native desktop performance or complete freeze resolution is claimed.

## Diagnostic input-method comparison

Using the previously recorded optimized binary, six editor cycles passed with process-local `GTK_IM_MODULE=simple`: 57.16 MB peak tracked heap, 261.55 MB peak RSS including Heaptrack overhead and 47.67 MB outstanding at graceful exit. The default-input-method run left 49.29 MB outstanding. The diagnostic setting therefore explains part of the observed retention, but is not a product fix and does not replace multilingual input support. GTK documents the setting in [Running and debugging GTK applications](https://gnome.pages.gitlab.gnome.org/gtk/gtk4/running.html).

Raw traces, native control scripts and failing/passing logs remain in the persistent local `bookie-test` profiling directory. All interaction used isolated Xvfb. No native desktop input or capture occurred.
