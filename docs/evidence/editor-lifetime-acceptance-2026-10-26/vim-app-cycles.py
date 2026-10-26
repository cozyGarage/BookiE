import json,runpy,sys,time
from pathlib import Path
sys.path.insert(0,str(Path.cwd()/'apps/linux/app/tests'))
import gtk_safety as ui
import gtk_ux
original_fixture=ui.write_fixture
original_run=ui.run_scenario
def fixture(*args,**kwargs):
    database,environment=original_fixture(*args,**kwargs)
    path=Path(environment['XDG_CONFIG_HOME'])/ui.storage_dir_name()/'preferences.json'
    preferences=json.loads(path.read_text())
    preferences['editor_vim_mode']=True
    path.write_text(json.dumps(preferences))
    return database,environment
def run(binary,scenario):
    def probe(database,base):
        editor=ui.set_editor_text('SELECT 123;')
        bounds=editor.queryComponent().getExtents(ui.pyatspi.WINDOW_COORDS)
        gtk_ux.x11_click(bounds.x+60,bounds.y+10,button=1)
        time.sleep(.3)
        ui.press_x11_key('Escape')
        ui.press_x11_key('End')
        time.sleep(.3)
        before=editor.queryText().caretOffset
        assert before>0
        ui.press_x11_key('h')
        deadline=time.monotonic()+5
        while editor.queryText().caretOffset != before-1 and time.monotonic()<deadline:
            time.sleep(.05)
        assert editor.queryText().getText(0,editor.queryText().characterCount)=='SELECT 123;'
        assert editor.queryText().caretOffset==before-1,'Vim normal-mode motion did not move the caret'
        print('Verified live Vim normal-mode motion before repeated editor closes.',flush=True)
        scenario(database,base)
    probe.environment=scenario.environment
    probe.__name__='vim_'+scenario.__name__
    return original_run(binary,probe)
ui.write_fixture=fixture
ui.run_scenario=run
runpy.run_path(str(Path(__file__).with_name('app-cycles.py')),run_name='__main__')
