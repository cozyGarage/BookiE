import os,runpy,sys
from pathlib import Path
sys.path.insert(0,str(Path.cwd()/'apps/linux/app/tests'))
import gtk_safety as ui
original=ui.write_fixture
def fixture(*args,**kwargs):
    database,environment=original(*args,**kwargs)
    environment['LD_PRELOAD']='/usr/lib/heaptrack/libheaptrack_preload.so'
    environment['DUMP_HEAPTRACK_OUTPUT']=os.environ['BOOKIE_HEAPTRACK_RAW']
    return database,environment
ui.write_fixture=fixture
runpy.run_path(str(Path(__file__).with_name('refusal-repeat.py')),run_name='__main__')
