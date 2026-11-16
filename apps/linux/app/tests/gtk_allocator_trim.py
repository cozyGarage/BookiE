import os
from pathlib import Path
import subprocess


def preload(environment):
    if not environment.get("BOOKIE_PROFILE_MALLOC_TRIM_RESULT"):
        return None
    runtime = Path(environment["XDG_RUNTIME_DIR"])
    source = Path(__file__).with_name("malloc_trim_probe.c")
    library = runtime / "bookie-malloc-trim.so"
    if not library.is_file() or library.stat().st_mtime_ns < source.stat().st_mtime_ns:
        subprocess.run(
            ["cc", "-Wall", "-Wextra", "-Werror", "-shared", "-fPIC", "-pthread", str(source), "-o", str(library)],
            check=True,
            capture_output=True,
            text=True,
        )
    return str(library)
