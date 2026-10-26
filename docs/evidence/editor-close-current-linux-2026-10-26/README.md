# Editor close and GTK text-buffer resource check, 2026-10-26

Related: [#1299](https://github.com/cozyGarage/BookiE/issues/1299) and [#1333](https://github.com/cozyGarage/BookiE/issues/1333). These runs are headless Xvfb/Cairo diagnostics, not native Wayland acceptance.

## Setup

The profiled application source snapshot is `9d82a5ebef7455d8ff28bb3289ffcab144947f7c`. The installed CI-profile app binary has SHA-256 `f99163ce404f8c3a00ccf913e07cf00f22dcd0bfa29e1c902e2a3039a1ba5e9f`. The host reports GTK 4.24.1, libadwaita 1.10.0 and GtkSourceView 5.22.0.

The app scenario opens four SQL editor tabs, fills each with about 1.1 MB of comment-heavy SQL, closes all four tabs, opens another editor and verifies a query. It runs for 18 cycles. After the last query, the process is sampled over a 15-second idle window. GTK's default input-method behavior (`GTK_IM_MODULE` unset) is compared with `GTK_IM_MODULE=simple`; the latter is only a diagnostic control.

A separate GTK-only control initializes GtkSourceView, creates four widgets, assigns each the same-size text, removes and drops the widgets, then repeats 18 times. It has no BookiE code or query activity. Both app and control use the same local GTK stack, private D-Bus session, Xvfb and Cairo rendering.

## Results

| Scenario | Input method | Initial RSS | RSS after 18 cycles | RSS after idle | Idle CPU |
|---|---|---:|---:|---:|---:|
| BookiE editor lifecycle | Default | 174.0 MiB | 446.2 MiB | 448.9 MiB | 0.00% of one core |
| BookiE editor lifecycle | Simple control | 172.6 MiB | 251.7 MiB | 248.0 MiB | 0.13% of one core |
| GTK-only SourceView lifecycle | Default | 108.7 MiB | 455.1 MiB | 455.1 MiB | 0.00% of one core |
| GTK-only SourceView lifecycle | Simple control | 108.6 MiB | 135.6 MiB | 135.6 MiB | 0.00% of one core |

The default-input-method RSS climb reproduces without BookiE, so this run does not support an app-only leak diagnosis. The simple-input-method control sharply reduces retention in both the app and GTK-only cases. The evidence shows that the observed resource growth changes with input-method configuration. It does not identify the selected default input-method module, the retained object, or whether memory remains reachable or is held by an allocator. The GTK settings property is unset in this Xvfb environment, so this is not allocation-stack attribution or proof of a GTK defect.

The app's idle CPU remains negligible after the workload. The reported tab-close freeze was not reproduced by this headless scenario. Do not force the simple input method in the product: this measurement does not establish that doing so is acceptable for users who rely on other input methods. Confirm on a native Wayland session and attribute allocations before making an app-side workaround.

## Records

- [Six-cycle paired scenario outcomes](results.json)
- [Six-cycle app samples, default input method](linux-9d82a5ebe/samples.json)
- [Six-cycle app samples, simple input method](linux-9d82a5ebe-simple-im/samples.json)
- [Eighteen-cycle app samples, default input method](linux-9d82a5ebe/default-idle-18/resource-idle-samples.json)
- [Eighteen-cycle default scenario outcome](linux-9d82a5ebe/default-idle-18/repeated_editor_close_cycles-result.json)
- [Eighteen-cycle app samples, simple input method](linux-9d82a5ebe/simple-idle-18/resource-idle-samples.json)
- [Eighteen-cycle simple-control scenario outcome](linux-9d82a5ebe/simple-idle-18/repeated_editor_close_cycles-result.json)
- [GTK-only lifecycle control](linux-9d82a5ebe/sourceview-lifecycle-control.py)
- [GTK-only default input-method result](linux-9d82a5ebe/sourceview-default/result.json)
- [GTK-only simple input-method result](linux-9d82a5ebe/sourceview-simple/result.json)

## Forgejo gate

Run #989 passed all 32 jobs at code commit `d3566e3b210b47334cca4113f264530933485223`. The [record](forgejo-result.json) identifies the gated source and comparison base. The branch's own app, crate and script diff remained identical after the subsequent Linux merge and documentation updates. This gate covers the query-allocation transfer, not the separate editor-buffer and Vim-context follow-up.
