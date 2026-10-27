import gc,json,time
from pathlib import Path
import gi
gi.require_version('Gtk','4.0')
from gi.repository import Gtk,Gio,GLib
Gtk.init()
context=GLib.MainContext.default()
finalized=[]
samples=[]
parent=Gtk.Window()
parent.present()
def pump(seconds):
    until=time.monotonic()+seconds
    while time.monotonic()<until:
        while context.pending():context.iteration(False)
        time.sleep(.01)
def finished(source,result,state):
    try:source.open_finish(result)
    except GLib.Error:pass
    state.append(True)
for cycle in range(18):
    state=[]
    dialog=Gtk.FileDialog(title='File dialog lifetime control')
    dialog.weak_ref(lambda:finalized.append(True))
    cancel=Gio.Cancellable()
    dialog.open(parent,cancel,finished,state)
    pump(.15)
    cancel.cancel()
    deadline=time.monotonic()+5
    while not state and time.monotonic()<deadline:pump(.01)
    assert state,'cancel callback did not complete'
    del dialog,cancel
    gc.collect()
    pump(.1)
    rss=next(line for line in Path('/proc/self/status').read_text().splitlines() if line.startswith('VmRSS:'))
    samples.append({'cycle':cycle+1,'finalized':len(finalized),'rss':rss})
parent.destroy()
print(json.dumps(samples,indent=2))
