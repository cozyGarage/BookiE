import gc,json,time
from pathlib import Path
import gi
gi.require_version('Gtk','4.0')
from gi.repository import Gtk,GLib
Gtk.init()
context=GLib.MainContext.default()
finalized=[]
samples=[]
def pump(seconds):
    until=time.monotonic()+seconds
    while time.monotonic()<until:
        while context.pending(): context.iteration(False)
        time.sleep(.01)
for cycle in range(18):
    window=Gtk.Window()
    chooser=Gtk.FileChooserWidget(action=Gtk.FileChooserAction.OPEN)
    chooser.weak_ref(lambda: finalized.append(True))
    window.set_child(chooser)
    window.present()
    pump(.15)
    window.set_child(None)
    window.destroy()
    del chooser,window
    gc.collect()
    pump(.15)
    rss=next(line for line in Path('/proc/self/status').read_text().splitlines() if line.startswith('VmRSS:'))
    samples.append({'cycle':cycle+1,'finalized':len(finalized),'rss':rss})
assert len(samples)==18
print(json.dumps(samples,indent=2))
