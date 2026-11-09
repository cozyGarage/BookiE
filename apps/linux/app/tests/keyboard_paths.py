"""Keyboard path for every clickable control on a surface.

Each accessible name maps to how a keyboard user reaches the same action:
"shortcut: ...", "menu: ...", "tab-stop", or "gap: ..." for a known missing
path. A clickable that is not listed fails the scenario, so a new
mouse-only control cannot appear unnoticed.
"""

PATH_KINDS = ("shortcut:", "menu:", "tab-stop", "gap:")

SQL_EDITOR = {
    "Run": "shortcut: Ctrl+Enter",
    "Explain": "menu: Main menu > Explain query",
    "Fold SQL blocks": "tab-stop",
    "Fold block": "shortcut: Ctrl+Shift+[",
    "Unfold all": "shortcut: Ctrl+Shift+]",
    "SQL line bookmarks": "tab-stop",
    "Toggle bookmark": "shortcut: Ctrl+F2",
    "Next bookmark": "shortcut: Ctrl+Shift+Down",
    "Previous bookmark": "shortcut: Ctrl+Shift+Up",
    "Clear bookmarks": "tab-stop",
    "Session": "tab-stop",
    "Pin results": "tab-stop",
    "Compare\u2026": "tab-stop",
    "Split": "tab-stop",
    "New query (Ctrl+E)": "shortcut: Ctrl+E",
    "Close Tab": "shortcut: Ctrl+W",
    "New connection": "tab-stop",
    "Open saved connection": "shortcut: Ctrl+P (Open Quickly lists connections)",
    "Main menu": "shortcut: F10",
    "SQL snippets": "tab-stop",
    "Search tables": "tab-stop",
    "Refresh tables": "shortcut: Ctrl+Shift+R",
    "Collapse all": "tab-stop",
    "Expand all": "tab-stop",
    "Tables sidebar": "tab-stop",
    "Run options": "tab-stop",
    "SQL snippets": "tab-stop",
    "Show columns": "tab-stop",
    "Results": "shortcut: Ctrl+J",
    "Errors": "tab-stop",
    "Maximize results": "tab-stop",
    "New Table\u2026": "tab-stop",
    "Table from CSV\u2026": "tab-stop",
    "Open safety_items": "tab-stop (Enter on the sidebar row)",
    "Close": "shortcut: Ctrl+Q",
    "Minimize": "tab-stop",
    "Maximize": "tab-stop",
    "Minimise": "tab-stop",
    "Maximise": "tab-stop",
}


def unregistered(names, registry):
    return sorted({name for name in names if name not in registry})


def malformed(registry):
    return sorted(name for name, path in registry.items() if not path.startswith(PATH_KINDS))
