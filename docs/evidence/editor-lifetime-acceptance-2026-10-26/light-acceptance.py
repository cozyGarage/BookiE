import json,sys
from pathlib import Path
sys.path.insert(0,str(Path.cwd()/'apps/linux/app/tests'))
import gtk_safety as ui
original=ui.write_fixture
def fixture(*args,**kwargs):
    database,environment=original(*args,**kwargs)
    path=Path(environment['XDG_CONFIG_HOME'])/ui.storage_dir_name()/'preferences.json'
    prefs=json.loads(path.read_text())
    prefs['color_scheme']='light'
    path.write_text(json.dumps(prefs))
    return database,environment
ui.write_fixture=fixture
ui.main()
