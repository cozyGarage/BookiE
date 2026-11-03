# Tauri result-grid experiment

This isolated experiment compares a welcome layout and a virtualized result grid with the GTK application. It uses synthetic values only and cannot connect to a database. It is not a supported frontend, a package, or an alternative execution path. The shipped application continues to use GTK. The experiment does not establish accessibility, input-method, security, cross-platform or performance parity.

The Rust command uses a bounded request and the shared `bookie-core` value type for its small synthetic fixture. Its JSON envelope represents integers as decimal strings and distinguishes null, booleans and text. Other database types are deliberately unsupported. This is not a complete value bridge, and no policy or database operations cross it.

From the repository root:

```bash
node --test experiments/tauri-grid-prototype/frontend/grid.test.mjs
cargo test --manifest-path experiments/tauri-grid-prototype/src-tauri/Cargo.toml --locked
cargo run --manifest-path experiments/tauri-grid-prototype/src-tauri/Cargo.toml --locked
```

Select **Open sample result** to display one million synthetic rows. Scroll to inspect different ranges. Only a small window of rows is requested at a time; the prototype caps both requests and the scroll surface. Do not use measurements from this synthetic screen as a comparison against BookiE's database-driven grid.

Before considering a second frontend, evaluate real application integration, exact values across every variant, authorization, audit, cancellation, clipboard, keyboard navigation, screen readers, input methods, themes, scaling, operating-system packaging and installed performance. Changes to the product's Linux-only UI decision require a separate accepted decision.
