"""Order-dependent dialog sequences: a dialog that is dismissed must be released, not just hidden.

AT-SPI cannot see a dismissed dialog that is still referenced, so the app writes a leak report
(see ui/leak_probe.rs) when asked through `win.report-leaks`.
"""
import time
from pathlib import Path

# key -> (action that opens it, accessible title, name the app tracks it under)
DIALOGS = {
    "preferences": ("win.preferences", "Preferences"),
    "shortcuts": ("win.shortcuts", "Keyboard Shortcuts"),
    "about": ("win.about", "About"),
    "catalog": ("win.show-catalog", "Catalog"),
    "audit": ("win.show-audit", "Audit Log"),
    "saved_queries": ("win.show-saved-queries", "Saved Queries"),
    "quick_switcher": ("win.open-quickly", "Open quickly"),
}
CYCLES = 4


def scenarios(ui):
    dialog_roles = (ui.pyatspi.ROLE_DIALOG, ui.pyatspi.ROLE_FRAME)
    def leak_report(base):
        path = Path(base) / "leak-report.txt"
        path.unlink(missing_ok=True)
        ui.invoke_accessible_action("win.report-leaks")
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while not path.exists():
            assert time.monotonic() < deadline, "the app did not write a leak report"
            time.sleep(ui.POLL_SECONDS)
        return dict(line.split("=") for line in path.read_text().splitlines())

    def close_with_own_control(dialog):
        actions = dialog.queryAction()
        for index in range(actions.nActions):
            if actions.getName(index) == "window.close":
                return actions.doAction(index)
        for node in ui.descendants(dialog):
            if ui.node_name(node) == "Close" and ui.node_role(node) == ui.pyatspi.ROLE_PUSH_BUTTON:
                return ui.invoke(node)
        return False

    def dismiss(title):
        # Escape is dropped when focus is not inside the dialog, so use the dialog's own close control.
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while (dialog := ui.find_node(name=title, role=dialog_roles)) is not None:
            assert time.monotonic() < deadline, f"{title!r} did not close:\n{ui.accessible_snapshot()}"
            if not close_with_own_control(dialog):
                ui.press_x11_key("Escape")
            time.sleep(0.5)

    def make_scenario(key):
        action, title = DIALOGS[key]

        def scenario(_database, base):
            for index in range(CYCLES):
                ui.invoke_accessible_action(action)
                ui.wait_for_node(name=title)
                dismiss(title)
                # Closing animations and idle callbacks release a dialog shortly after it hides.
                deadline = time.monotonic() + ui.WAIT_SECONDS
                while True:
                    report = leak_report(base)
                    if int(report.get(key, 0)) == 0 or time.monotonic() > deadline:
                        break
                    time.sleep(ui.POLL_SECONDS)
                assert int(report.get(key, 0)) == 0, (
                    f"{key} dialog is still alive after dismissal {index + 1}: {report}\n"
                    f"{ui.accessible_snapshot()}"
                )

        scenario.__name__ = f"{key}_dialog_is_released_after_repeated_open_and_dismiss"
        return scenario

    return [make_scenario(key) for key in DIALOGS]
