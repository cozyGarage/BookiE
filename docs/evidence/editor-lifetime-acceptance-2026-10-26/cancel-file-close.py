import json,subprocess,sys
from pathlib import Path
subprocess.run(['gdbus','call','--session','--dest','org.a11y.Bus','--object-path','/org/a11y/bus','--method','org.a11y.Bus.GetAddress'],check=True,stdout=subprocess.DEVNULL)
sys.path.insert(0,str(Path.cwd()/'apps/linux/app/tests'))
import gtk_safety as ui

def cancel_preserves_dirty_file_and_later_save(_database,base,restart):
    path=base/'cancel-preserved.sql'
    original='SELECT 1;\n'
    path.write_text(original)
    def seed(environment):
        state=Path(environment['XDG_CONFIG_HOME'])/ui.storage_dir_name()/'workspace_state.json'
        state.write_text(json.dumps({'connections':{'d4f5e246-c40a-4a30-90f2-39e93e94d920':{'tabs':[{'kind':'editor','query':'','draft_id':None,'file':str(path)}],'active_idx':0}}}))
    restart(before_start=seed,restored=True)
    ui.wait_for_node_containing(path.name,role=ui.pyatspi.ROLE_PAGE_TAB)
    editor=ui.set_editor_text('SELECT 456;')
    ui.invoke_accessible_action('win.close-current')
    ui.invoke(ui.wait_for_node(name='Cancel',role=ui.pyatspi.ROLE_PUSH_BUTTON))
    ui.wait_for_node(name='Discard',role=ui.pyatspi.ROLE_PUSH_BUTTON,present=False)
    ui.wait_for_node_containing(path.name,role=ui.pyatspi.ROLE_PAGE_TAB)
    assert editor.queryText().getText(0,editor.queryText().characterCount)=='SELECT 456;'
    assert path.read_text()==original
    ui.invoke_accessible_action('win.close-current')
    ui.invoke(ui.wait_for_node(name='Save',role=ui.pyatspi.ROLE_PUSH_BUTTON))
    ui.wait_for_node_containing(path.name,role=ui.pyatspi.ROLE_PAGE_TAB,present=False)
    assert path.read_text()=='SELECT 456;'
    ui.wait_for_node(name='Open SQL editor',role=ui.pyatspi.ROLE_PUSH_BUTTON)
cancel_preserves_dirty_file_and_later_save.environment='local'
cancel_preserves_dirty_file_and_later_save.needs_restart=True
ui.run_scenario(Path(sys.argv[1]),cancel_preserves_dirty_file_and_later_save)
