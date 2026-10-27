import json
import os
import subprocess
import sys
import time
from pathlib import Path

subprocess.run([
    "gdbus", "call", "--session", "--dest", "org.a11y.Bus", "--object-path", "/org/a11y/bus",
    "--method", "org.a11y.Bus.GetAddress",
], check=True, stdout=subprocess.DEVNULL)
sys.path.insert(0, str(Path.cwd() / "apps/linux/app/tests"))
import gtk_safety as ui

output = Path(os.environ["TABLEPRO_GTK_ARTIFACT_DIR"])
os.environ["TABLEPRO_PROFILE_OUT"] = str(output / "refusal-cycles.jsonl")
samples = []


def sample(phase):
    app = ui.owned_application()
    identity = app.identity()
    rss, peak = app.memory_kb()
    fields = Path(f"/proc/{identity['application_pid']}/stat").read_text().rsplit(")", 1)[1].split()
    result = {
        **identity, "phase": phase, "monotonic": time.monotonic(), "rss_kib": rss,
        "peak_rss_kib": peak, "cpu_ticks": int(fields[11]) + int(fields[12]),
    }
    assert app.identity() == identity
    samples.append(result)
    return result


def repeated_overlong_file_refusal(database, base):
    sample("initial")
    for batch in range(6):
        ui.overlong_sql_files_are_refused_without_replacing_the_editor(database, base)
        result = sample(f"batch-{batch + 1}-complete")
        print(f"batch {batch + 1} complete: {result['rss_kib']} KiB RSS", flush=True)
    before = sample("idle-start")
    time.sleep(15)
    after = sample("idle-end")
    seconds = after["monotonic"] - before["monotonic"]
    cpu = (after["cpu_ticks"] - before["cpu_ticks"]) / os.sysconf("SC_CLK_TCK")
    samples.append({"phase": "idle-summary", "seconds": seconds, "cpu_seconds": cpu, "one_core_percent": 100 * cpu / seconds})
    app = ui.owned_application()
    ui.invoke_accessible_action("win.quit")
    assert app.process.wait(timeout=30) == 0


repeated_overlong_file_refusal.environment = "local"
try:
    ui.run_scenario(Path(sys.argv[1]), repeated_overlong_file_refusal)
finally:
    (output / "samples.json").write_text(json.dumps(samples, indent=2) + "\n")
