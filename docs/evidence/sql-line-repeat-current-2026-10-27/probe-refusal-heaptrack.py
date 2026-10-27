import os
import sys
from pathlib import Path

repo = Path(os.environ["TABLEPRO_PROFILE_REPO"])
sys.path.insert(0, str(repo / "apps/linux/app/tests"))
import gtk_safety as ui

original_start = ui.start_application

def start_with_heaptrack(binary, environment, restored=False):
    environment = environment.copy()
    environment["LD_PRELOAD"] = "/usr/lib/heaptrack/libheaptrack_preload.so"
    environment["DUMP_HEAPTRACK_OUTPUT"] = str(Path(os.environ["TABLEPRO_PROFILE_HEAP_DIR"]) / "refusal.raw")
    return original_start(binary, environment, restored)

ui.start_application = start_with_heaptrack
ui.main()
