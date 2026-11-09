# 0039: GPUI source for the experimental frontends

- **Status**: Accepted (2026-11-09)
- **Date**: 2026-11-05

## Context

ADR 0028 allows isolated frontend experiments. The GPUI prototype pinned crates.io `gpui =0.2.2` (published 2025-10-22). Zed does not publish newer releases of that crate, so 0.2.2 is a year behind upstream. The maintainer asked for a source that tracks upstream.

Reviewed sources, from the crates.io and GitHub APIs and the gpui-archipelago data repositories:

- `gpui-pre` (weekly Zed snapshots, 0.3.11 on 2026-11-02) through `gpui-kit`, which bundles `gpui-component`. Used by the active projects zedis, based, postman-gpui and dbflux (vendored and patched).
- `gpui-unofficial` (republishes each Zed release tag). No reviewed application uses it.
- `gpui-ce` (community fork, API diverging from Zed), `gpui-fast`, and a raw Zed git revision.

`gpui-component 0.9.0` pins `gpui-pre =0.3.11`; each release moves the pin. Two forks cannot share one application. All of these are third-party republishes of Zed code.

## Decision

Experiments use `gpui-pre`, `gpui-pre-platform`, `gpui-pre-macros` and `gpui-kit` as an exact pair, bumped together. Zed git revisions and the other forks are not used. This replaces the `gpui =0.2.2` pin.

## Evidence

The GPUI prototype was ported on branch `exp/gpui-component-spike`:

- The renderer needed 19 compile fixes, all mechanical: `Window::focus` and `FocusHandle::focus` take `cx`, `flex_grow` and `flex_shrink` take a factor, `Application::new()` became `gpui_platform::application()`, `TextLayout::paint` takes alignment, `track_scroll` takes a reference.
- All 77 tests pass, including the 19 headless interaction tests on `test-support`.
- `cargo tree -i gpui-pre` shows one GPUI in the tree. About 720 crates; a release build took about 5 minutes here and the debug target is 5.9 GB.
- The release binary opens a window under Wayland on the maintainer's desktop (found by process id).

Not verified: a screenshot of the new build, X11, performance against 0.2.2, and the `gpui-kit` licence text (crates.io lists Apache-2.0, GitHub shows no assertion).

## Consequences

- Every `gpui-kit` release can break the build; bump the pair deliberately and re-run the tests.
- The heavy dependency graph stays inside `experiments/`, outside the shipped workspace and its `cargo deny` policy.
- This does not decide the production frontend. The GTK application stays the product.
- If a component from `gpui-kit` is adopted, record it in the adoption notes; copied code from other projects goes in `docs/adopted-code-attribution.md` with its licence.
