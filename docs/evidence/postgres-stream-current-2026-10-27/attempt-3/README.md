# Heaptrack attempt

The release binary was built from Linux `9cad64e64d9c64faaa4da3946c0e1ebdd35a8023` and has SHA-256 `ff09cb10448710f3c121c22a79e4057bc62a7270eaa8b5ad101738adafcc8e35`. Heaptrack 1.5.0 instrumented the app during a six-cycle PostgreSQL workload.

The first query did not reach a terminal UI state within the probe's 180-second deadline. The scenario failed after 192.054 seconds. This is an instrumentation-limited attempt, not evidence that the uninstrumented query regressed. Its incomplete trace was discarded; no complete allocation profile is available.
