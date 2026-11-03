import json
import os
import re
import time

import pyatspi

import gtk_ux

PAGE_ROWS = 5000
NEEDLE = "item-14999"


def stream_rows():
    rows = int(os.environ.get("BOOKIE_GTK_STREAM_ROWS", "150000"))
    assert rows >= 2 * PAGE_ROWS and rows % PAGE_ROWS == 0
    return rows


def left_sql(rows):
    return (
        "SELECT value AS id, 'item-' || value AS label, repeat('x', 2048) AS payload "
        f"FROM generate_series(1, {rows}) AS value"
    )


def right_sql(rows, added):
    return (
        "SELECT value AS id, CASE WHEN value % 100 = 3 THEN 'edited-' || value ELSE 'item-' || value END AS label, "
        f"repeat('x', 2048) AS payload FROM generate_series(1, {rows + added}) AS value WHERE value > {rows} OR value % 100 <> 2"
    )


def expected_counts(rows):
    added = rows // 40
    removed = sum(1 for value in range(1, rows + 1) if value % 100 == 2)
    changed = sum(1 for value in range(1, rows + 1) if value % 100 == 3)
    return {
        "added": added,
        "removed": removed,
        "changed": changed,
        "equal": rows - removed - changed,
        "right_rows": rows - removed + added,
    }


def needle_ids(rows, label):
    return [value for value in range(1, rows + 1) if NEEDLE in label(value)]


def scenarios(ui):
    def showing(node):
        return node.getState().contains(pyatspi.STATE_SHOWING)

    def wait_visible_label(text, timeout=30):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            application = ui.application_node()
            if application is not None and any(
                ui.node_name(node) == text and ui.node_role(node) == pyatspi.ROLE_LABEL and showing(node)
                for node in ui.descendants(application)
            ):
                return
            time.sleep(ui.POLL_SECONDS)
        raise AssertionError(f"label {text!r} was not visible:\n{ui.accessible_snapshot()}")

    def editable_named(name, timeout=30):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            for application in ui.desktop_applications():
                for node in ui.descendants(application):
                    if ui.node_name(node) != name:
                        continue
                    try:
                        node.queryEditableText()
                    except Exception:
                        continue
                    return node
            time.sleep(ui.POLL_SECONDS)
        raise AssertionError(f"no editable node named {name!r}:\n{ui.accessible_snapshot()}")

    def search_for(needle):
        entry = editable_named("Find in all rows")
        entry.queryEditableText().setTextContents(needle)
        extents = entry.queryComponent().getExtents(pyatspi.WINDOW_COORDS)
        gtk_ux.x11_click(extents.x + extents.width // 2, extents.y + extents.height // 2, button=1)
        time.sleep(0.4)
        ui.press_x11_key("Return")

    def count_named(name, role):
        return sum(
            1
            for application in ui.desktop_applications()
            for node in ui.descendants(application)
            if ui.node_name(node) == name and ui.node_role(node) == role
        )

    def open_editor():
        ui.open_saved_connection(ui.POSTGRES_CONNECTION_NAME)
        ui.wait_for_frame_containing(f"{ui.POSTGRES_CONNECTION_NAME} — BookiE")
        ui.invoke(ui.wait_for_node(name="Open SQL editor"))
        ui.wait_for_node(name="Run", role=pyatspi.ROLE_PUSH_BUTTON)

    def find_steps(rows):
        ids = needle_ids(rows, lambda value: f"item-{value}")
        total = len(ids)
        assert total >= 3, ids
        search_for(NEEDLE)
        ui.wait_for_node_containing(f"Match 1 of {total}", timeout=120)
        wait_visible_label(f"item-{ids[0]}")
        ui.invoke(ui.wait_for_node(name="Next match", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node_containing(f"Match 2 of {total}")
        wait_visible_label(f"item-{ids[1]}")
        ui.invoke(ui.wait_for_node(name="Previous match", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node_containing(f"Match 1 of {total}")
        wait_visible_label(f"item-{ids[0]}")
        ui.invoke(ui.wait_for_node(name="Previous match", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node_containing(f"Match {total} of {total}")
        wait_visible_label(f"item-{ids[-1]}")
        return total

    def filter_steps(total):
        ui.invoke(ui.wait_for_node(name="Show matching rows", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node_containing(f"Complete · Received {total:,} rows", timeout=120)
        ui.wait_for_node_containing("FILTER " + NEEDLE)
        ui.wait_for_node(name="Export all rows…", role=pyatspi.ROLE_PUSH_BUTTON)

    def compare_steps(counts):
        ui.invoke(ui.wait_for_sensitive_node(name="Compare…", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node(name="Compare results", role=(pyatspi.ROLE_DIALOG, pyatspi.ROLE_ALERT, pyatspi.ROLE_FRAME))
        ui.wait_for_node(name="Left result")
        ui.wait_for_node(name="Right result")
        key = ui.wait_for_node(name="id", role=pyatspi.ROLE_CHECK_BOX)
        extents = key.queryComponent().getExtents(pyatspi.WINDOW_COORDS)
        gtk_ux.x11_click(extents.x + 40, extents.y + extents.height // 2, button=1)
        time.sleep(1.0)
        assert key.getState().contains(pyatspi.STATE_CHECKED), "the key column was not selected"
        ui.invoke(ui.wait_for_node(name="Compare", role=pyatspi.ROLE_PUSH_BUTTON))
        summary = (
            f"Equal {counts['equal']:,} · Added {counts['added']:,} · "
            f"Removed {counts['removed']:,} · Changed {counts['changed']:,}"
        )
        ui.wait_for_node_containing(summary, timeout=300)

    def compare_audit(base, counts):
        journal = base / "data" / ui.storage_dir_name() / "audit.jsonl"
        deadline = time.monotonic() + ui.WAIT_SECONDS
        events = []
        while time.monotonic() < deadline:
            if journal.exists():
                events = [json.loads(line)["event"] for line in journal.read_text().splitlines() if line.strip()]
                compare = [event for event in events if (event.get("administrative_action") or {}).get("action") == "result_compare"]
                if len(compare) >= 2:
                    break
            time.sleep(ui.POLL_SECONDS)
        compare = [event for event in events if (event.get("administrative_action") or {}).get("action") == "result_compare"]
        assert len(compare) == 2, f"expected one intent and one outcome, found {compare!r}"
        intent, outcome = compare
        assert intent["phase"] == "intent" and outcome["phase"] == "outcome", compare
        assert intent["operation_id"] == outcome["operation_id"], compare
        assert outcome["terminal_status"] == "succeeded", outcome
        assert outcome["operation_class"] == "read", outcome
        assert outcome["connection_name"] == ui.POSTGRES_CONNECTION_NAME, outcome
        differing = counts["added"] + counts["removed"] + counts["changed"]
        assert outcome["administrative_action"]["affected_count"] == differing, outcome
        encoded = json.dumps(compare)
        for forbidden in ("item-", "edited-", "xxxxxxxx"):
            assert forbidden not in encoded, f"the compare audit records leaked row data: {forbidden!r}"

    def postgres_whole_result_find_filter_and_compare(database, base):
        rows = stream_rows()
        counts = expected_counts(rows)
        open_editor()
        ui.run_sql(left_sql(rows))
        ui.wait_for_node_containing(f"Complete · Received {rows:,} rows", timeout=300)
        scheme = os.environ.get("ADW_DEBUG_COLOR_SCHEME", "prefer-light")
        gtk_ux.capture_grid_navigation_artifact(f"whole-result-{rows}-{scheme}-complete")
        total = find_steps(rows)
        gtk_ux.capture_grid_navigation_artifact(f"whole-result-{rows}-{scheme}-find")
        filter_steps(total)
        gtk_ux.capture_grid_navigation_artifact(f"whole-result-{rows}-{scheme}-filtered")
        ui.invoke(ui.wait_for_node(name="Unpin results (1)", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.invoke(ui.wait_for_sensitive_node(name="Pin results", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node(name="Unpin results (1)", role=pyatspi.ROLE_PUSH_BUTTON)
        ui.run_sql(right_sql(rows, counts["added"]))
        ui.wait_for_node_containing(f"({counts['right_rows']:,})", timeout=300)
        compare_steps(counts)
        gtk_ux.capture_grid_navigation_artifact(f"whole-result-{rows}-{scheme}-compared")
        compare_audit(base, counts)

    result = []
    if os.environ.get("BOOKIE_GTK_POSTGRES_PORT"):
        result.append(postgres_whole_result_find_filter_and_compare)
    for scenario in result:
        scenario.environment = "local"
    return result
