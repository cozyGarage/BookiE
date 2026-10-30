# Manual verification checklist

This checklist describes installed and native desktop scenarios that automated GTK tests cannot fully establish. It is not milestone status; the completed release boundary is recorded in the [sprint](bookie-0.2-sprint.md), and open work is tracked in [Project #3](https://github.com/users/cozyGarage/projects/3). The checkboxes summarize the recorded state at this snapshot.

Test native Wayland separately from Xvfb; an Xvfb pass does not establish native Wayland behavior.

For each run, record the binary/package identity, GTK backend, OS and toolkit versions, scale, theme, results and screenshots. Test normal and narrow window sizes, keyboard focus, clipboard and popover/dialog placement. Mark blocked and not-run items explicitly. This checklist defines scenarios; record each run separately.

Independent value checks compare database-native results with the app's display and supported write paths. The [oracle review](oracle-review.md) explains the database-native comparison method.

## Areas

| Area | Checklist |
|---|---|
| Connections and ssh | [manual-verification-connections.md](manual-verification-connections.md) |
| Sql editor and sessions | [manual-verification-editor.md](manual-verification-editor.md) |
| Grid, filters and data transfer | [manual-verification-grid.md](manual-verification-grid.md) |
| Catalog, sidebar and application shell | [manual-verification-app.md](manual-verification-app.md) |
