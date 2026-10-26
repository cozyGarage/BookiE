import gc
import json
import os
import time
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("GtkSource", "5")
from gi.repository import GLib, Gtk, GtkSource

Gtk.init()
GtkSource.init()
window = Gtk.Window()
stack = Gtk.Stack()
window.set_child(stack)
window.present()


def pump():
    main_context = GLib.MainContext.default()
    while main_context.pending():
        main_context.iteration(False)


def memory():
    status = Path(f"/proc/{os.getpid()}/status").read_text().splitlines()
    rss = next(int(line.split()[1]) for line in status if line.startswith("VmRSS:"))
    stat = Path(f"/proc/{os.getpid()}/stat").read_text().split()
    cpu_ticks = int(stat[13]) + int(stat[14])
    return {"rss_kib": rss, "cpu_ticks": cpu_ticks}


text = "-- control payload without BookiE callbacks\n" * 25_000
samples = [{"phase": "initial", "monotonic": time.monotonic(), **memory()}]
for cycle in range(18):
    pages = []
    for index in range(4):
        buffer = GtkSource.Buffer()
        view = GtkSource.View.new_with_buffer(buffer)
        page = Gtk.ScrolledWindow()
        page.set_child(view)
        stack.add_named(page, f"{cycle}-{index}")
        buffer.set_text(text)
        pages.append(page)
    pump()
    samples.append({"phase": f"cycle-{cycle}-open", "monotonic": time.monotonic(), **memory()})
    for page in pages:
        stack.remove(page)
    del pages, page, buffer, view
    gc.collect()
    pump()
    samples.append({"phase": f"cycle-{cycle}-closed", "monotonic": time.monotonic(), **memory()})

start = time.monotonic()
start_cpu = memory()["cpu_ticks"]
time.sleep(15)
end = time.monotonic()
end_mem = memory()
samples.append({"phase": "idle-end", "monotonic": end, **end_mem})
summary = {
    "input_method": os.environ.get("GTK_IM_MODULE", "default"),
    "cycles": 18,
    "views_per_cycle": 4,
    "payload_bytes_per_view": len(text.encode()),
    "idle_seconds": round(end - start, 4),
    "idle_cpu_seconds": round((end_mem["cpu_ticks"] - start_cpu) / os.sysconf("SC_CLK_TCK"), 4),
    "samples": samples,
}
print(json.dumps(summary))
window.destroy()
