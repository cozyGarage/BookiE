# Checkout interruption, 2026-10-26

Forgejo run 993, unit job, failed before compilation or test execution. The job received its task at 21:28:04 UTC and reported `RUN signal: terminated` at 21:28:20 UTC.

Read-only runner inspection matched that timestamp to systemd stopping `forgejo-runner.service`. The runner logged receipt of a shutdown signal and the service stopped successfully. No kernel entry was present in the inspected 21:27:50–21:29:00 UTC interval. The job reported 55,594,078,208 bytes free before and after cleanup, above its 48,318,382,080-byte reserve. A later filesystem check reported 52 GiB available.

The evidence identifies service shutdown as the immediate cause of this checkout interruption. It does not identify who requested the stop, why maintenance occurred, or prove application correctness. No product test failure was observed in this job. Read-only inspection did not restart the service or change runner configuration. Coordinate the service owner before retrying work on that runner.
