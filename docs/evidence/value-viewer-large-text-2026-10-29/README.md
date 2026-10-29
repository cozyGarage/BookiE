# Large value viewer responsiveness

Validation checkout: `360ae3985` (latest Linux merged into the fix branch). The change under review disables word/character wrapping for rendered values larger than 4 KiB.

## Reproduction and result

On GTK 4.24 under Xvfb, presenting a `GtkTextView` containing one unbroken 1 MiB line with `WordChar` did not complete within 45 seconds. The same view using `WrapMode::None` presented in 0.204 seconds. Smaller samples also showed a steep increase: 4 KiB took 0.74 seconds, 8 KiB took 2.94 seconds, and 16 KiB or larger did not finish within the 10-second probe timeout.

The app-level GTK/AT-SPI scenario `large_result_value_viewer_opens_without_stalling` inserts a 1,200,000-byte text value into SQLite, opens it from a SQL result, and waits for the viewer's Copy value button. It passed in 2.850 seconds end to end, with the viewer available before the 10-second limit.

The first app-level attempts failed because the test did not reliably invoke the grid context menu under Xvfb. The final scenario uses the existing keyboard path to focus the SQL result row and open its menu. The measured GTK text-layout probe independently reproduced the rendering bottleneck. A separate cross-process clipboard read was inconsistent under Xvfb and is not treated as copy evidence.

## Limits

The GTK-level timings are local debug-build measurements, not a release benchmark. The app-level test validates opening the SQL-result viewer under Xvfb; it does not validate clipboard contents. The reported native Wayland freeze and 66,124-byte clipboard result have not yet been retested on the current build.
