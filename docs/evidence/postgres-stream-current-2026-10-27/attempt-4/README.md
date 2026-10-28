# Current Linux profile

This release build uses application source from BookiE Linux `9cad64e64d9c64faaa4da3946c0e1ebdd35a8023`. Its binary SHA-256, toolchain and toolkit versions are in [build.json](build.json); all per-cycle process samples are in [profile-uninstrumented.json](profile-uninstrumented.json). The pinned PostgreSQL image and GTK probe are recorded in the build metadata and shared [profile script](../../postgres-stream-heap-profile-2026-10-25/run.sh).

Six two-million-row queries completed, each receiving all 400 pages and passing backward/forward page navigation. The query-time median was 9.065 seconds; cycle four took 16.282 seconds. RSS after closing the editor increased from 511.2 MiB after cycle one to 572.1 MiB after cycle six, then changed by 0.1 MiB between cycles five and six. This short sample suggests RSS was close to a plateau by cycle six. It does not prove long-session leak freedom or identify the retained allocations.

The run was uninstrumented. No Rust compiler was active at its start; system-wide timing conditions were not controlled. The profile is evidence for this source and setup only. The GTK scenario result is `passed` in [the result artifact](failure-artifacts/profile_postgres_large_stream-result.json).
