use std::io::{self, Write};
use std::rc::Rc;

use gtk4 as gtk;
use libadwaita::{self as adw, prelude::*};
use tablepro_core::Value;

use super::display::{value_to_display_text, value_to_search_text};
use crate::ui::row_object::RowObject;

const MAX_MATCHES: usize = 200;
const PREVIEW_CHARS: usize = 80;

pub(super) struct RowMatch {
    pub position: u32,
    pub column: usize,
    pub text: String,
}

struct CaseInsensitivePattern {
    characters: Vec<char>,
    prefix: Vec<usize>,
}

impl CaseInsensitivePattern {
    fn new(needle: &str) -> Self {
        let characters: Vec<char> = needle.chars().collect();
        let prefix = prefix_lengths(&characters);
        Self { characters, prefix }
    }

    fn matches(&self, text: &str) -> bool {
        let mut matched = 0;
        self.matches_from(&mut matched, text)
    }

    fn matches_from(&self, matched: &mut usize, text: &str) -> bool {
        for character in text.chars().flat_map(char::to_lowercase) {
            while *matched > 0 && character != self.characters[*matched] {
                *matched = self.prefix[*matched - 1];
            }
            if character == self.characters[*matched] {
                *matched += 1;
            }
            if *matched == self.characters.len() {
                return true;
            }
        }
        false
    }
}

struct JsonSearchWriter<'a> {
    pattern: &'a CaseInsensitivePattern,
    matched: usize,
    found: bool,
    pending: [u8; 4],
    pending_len: usize,
}

impl<'a> JsonSearchWriter<'a> {
    fn new(pattern: &'a CaseInsensitivePattern) -> Self {
        Self {
            pattern,
            matched: 0,
            found: false,
            pending: [0; 4],
            pending_len: 0,
        }
    }

    fn matches(&self) -> bool {
        self.found && self.pending_len == 0
    }

    fn scan(&mut self, text: &str) {
        self.found = self.pattern.matches_from(&mut self.matched, text);
    }
}

impl Write for JsonSearchWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(bytes.len());
        }
        if self.found {
            return Err(io::ErrorKind::Other.into());
        }
        let mut rest = bytes;
        if self.pending_len > 0 {
            let width = utf8_width(self.pending[0]).ok_or(io::ErrorKind::InvalidData)?;
            let needed = width - self.pending_len;
            let copied = needed.min(rest.len());
            self.pending[self.pending_len..self.pending_len + copied].copy_from_slice(&rest[..copied]);
            self.pending_len += copied;
            rest = &rest[copied..];
            if self.pending_len < width {
                return Ok(bytes.len());
            }
            let completed = self.pending;
            let text = std::str::from_utf8(&completed[..width]).map_err(io::Error::other)?;
            self.scan(text);
            self.pending_len = 0;
            if self.found {
                return Err(io::ErrorKind::Other.into());
            }
        }
        match std::str::from_utf8(rest) {
            Ok(text) => self.scan(text),
            Err(error) => {
                let end = error.valid_up_to();
                let text = std::str::from_utf8(&rest[..end]).map_err(io::Error::other)?;
                self.scan(text);
                if self.found {
                    return Err(io::ErrorKind::Other.into());
                }
                let incomplete = &rest[end..];
                if error.error_len().is_some() || incomplete.len() > 3 {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                self.pending[..incomplete.len()].copy_from_slice(incomplete);
                self.pending_len = incomplete.len();
            }
        }
        if self.found {
            return Err(io::ErrorKind::Other.into());
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn utf8_width(first: u8) -> Option<usize> {
    match first {
        0x00..=0x7f => Some(1),
        0xc2..=0xdf => Some(2),
        0xe0..=0xef => Some(3),
        0xf0..=0xf4 => Some(4),
        _ => None,
    }
}

fn matches_value(value: &Value, pattern: &CaseInsensitivePattern) -> bool {
    let Value::Json(json) = value else {
        return pattern.matches(&value_to_search_text(value));
    };
    let mut writer = JsonSearchWriter::new(pattern);
    let _ = serde_json::to_writer(&mut writer, json);
    writer.matches()
}

#[cfg(test)]
fn first_matching_cell(cells: &[Option<Value>], needle: &CaseInsensitivePattern) -> Option<(usize, String)> {
    cells
        .iter()
        .enumerate()
        .filter_map(|(column, value)| value.as_ref().map(|value| (column, value)))
        .find_map(|(column, value)| matches_value(value, needle).then(|| (column, value_to_display_text(value))))
}

fn prefix_lengths(pattern: &[char]) -> Vec<usize> {
    let mut prefix = vec![0; pattern.len()];
    let mut matched = 0;
    for index in 1..pattern.len() {
        while matched > 0 && pattern[index] != pattern[matched] {
            matched = prefix[matched - 1];
        }
        if pattern[index] == pattern[matched] {
            matched += 1;
        }
        prefix[index] = matched;
    }
    prefix
}

pub(super) fn matching_rows(
    count: u32,
    mut visit_row: impl FnMut(u32, &mut dyn FnMut(usize, &Value) -> bool),
    query: &str,
    limit: usize,
) -> Vec<RowMatch> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() || limit == 0 {
        return Vec::new();
    }
    let needle = CaseInsensitivePattern::new(&needle);
    let mut found = Vec::new();
    for position in 0..count {
        let mut matching_cell = None;
        visit_row(position, &mut |column, value| {
            if matches_value(value, &needle) {
                matching_cell = Some((column, value_to_display_text(value)));
                true
            } else {
                false
            }
        });
        if let Some((column, text)) = matching_cell {
            found.push(RowMatch { position, column, text });
            if found.len() == limit {
                break;
            }
        }
    }
    found
}

fn preview(text: &str) -> String {
    let single_line: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    if single_line.chars().count() <= PREVIEW_CHARS {
        return single_line;
    }
    let cut: String = single_line.chars().take(PREVIEW_CHARS).collect();
    format!("{cut}…")
}

pub(super) fn install(view: &gtk::ColumnView, actions: &gtk::gio::SimpleActionGroup, names: Vec<String>) {
    let action = gtk::gio::SimpleAction::new("find-row", None);
    let weak_view = view.downgrade();
    let names = Rc::new(names);
    action.connect_activate(move |_, _| {
        if let Some(view) = weak_view.upgrade().filter(|view| view.is_mapped()) {
            present(&view, names.clone());
        }
    });
    actions.add_action(&action);
    let controller = gtk::ShortcutController::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    controller.add_shortcut(
        gtk::Shortcut::builder()
            .trigger(&crate::ui::shortcut::parse("<Primary><Alt>f"))
            .action(&gtk::NamedAction::new("grid.find-row"))
            .build(),
    );
    view.add_controller(controller);
}

struct FindWidgets {
    content: gtk::Box,
    search: gtk::SearchEntry,
    status: gtk::Label,
    list: gtk::ListBox,
}

fn build_widgets() -> FindWidgets {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.append(&adw::HeaderBar::new());
    let search = gtk::SearchEntry::builder()
        .placeholder_text(crate::tr!("Search the loaded rows"))
        .margin_start(12)
        .margin_end(12)
        .build();
    search.update_property(&[gtk::accessible::Property::Label(&crate::tr!("Search the loaded rows"))]);
    content.append(&search);
    let status = gtk::Label::builder()
        .xalign(0.0)
        .margin_start(12)
        .margin_end(12)
        .build();
    status.add_css_class("dim-label");
    content.append(&status);
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .activate_on_single_click(true)
        .build();
    list.add_css_class("boxed-list");
    content.append(
        &gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .margin_start(12)
            .margin_end(12)
            .margin_bottom(12)
            .build(),
    );
    FindWidgets {
        content,
        search,
        status,
        list,
    }
}

fn result_row(label: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(
        &gtk::Label::builder()
            .label(label)
            .xalign(0.0)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .build(),
    ));
    row.update_property(&[gtk::accessible::Property::Label(label)]);
    row
}

fn status_text(query: &str, shown: usize) -> String {
    if query.trim().is_empty() {
        String::new()
    } else if shown == 0 {
        crate::tr!("No matching rows")
    } else if shown >= MAX_MATCHES {
        crate::tr!("The first {count} matches").replace("{count}", &MAX_MATCHES.to_string())
    } else {
        crate::tr!("{count} matching rows").replace("{count}", &shown.to_string())
    }
}

fn show_matches(widgets: &FindWidgets, model: &gtk::SelectionModel, names: &[String]) -> Vec<u32> {
    let query = widgets.search.text();
    let row_store = model
        .downcast_ref::<gtk::MultiSelection>()
        .and_then(|selection| selection.model())
        .and_then(|model| model.downcast::<crate::ui::row_store::RowStore>().ok());
    let found = matching_rows(
        model.n_items(),
        |position, visit| {
            if let Some(store) = row_store.as_ref() {
                store.visit_search_cells(position, visit);
            } else if let Some(row) = model.item(position).and_then(|item| item.downcast::<RowObject>().ok()) {
                row.visit_cells(visit);
            }
        },
        &query,
        MAX_MATCHES,
    );
    while let Some(row) = widgets.list.row_at_index(0) {
        widgets.list.remove(&row);
    }
    for hit in &found {
        let column = names.get(hit.column).map_or("", String::as_str);
        let label = format!("{} · {}: {}", hit.position + 1, column, preview(&hit.text));
        widgets.list.append(&result_row(&label));
    }
    widgets.list.select_row(widgets.list.row_at_index(0).as_ref());
    widgets.status.set_text(&status_text(&query, found.len()));
    found.iter().map(|found| found.position).collect()
}

fn close_on(dialog: &adw::Dialog) -> impl Fn() + 'static {
    let dialog = dialog.downgrade();
    move || {
        if let Some(dialog) = dialog.upgrade() {
            dialog.close();
        }
    }
}

fn present(view: &gtk::ColumnView, names: Rc<Vec<String>>) {
    let dialog = adw::Dialog::builder()
        .title(crate::tr!("Find in Loaded Rows"))
        .content_width(520)
        .content_height(460)
        .build();
    let widgets = Rc::new(build_widgets());
    let positions: Rc<std::cell::RefCell<Vec<u32>>> = Rc::default();
    widgets.search.connect_search_changed({
        let (view, widgets, positions) = (view.downgrade(), widgets.clone(), positions.clone());
        move |_| {
            if let Some(model) = view.upgrade().and_then(|view| view.model()) {
                *positions.borrow_mut() = show_matches(&widgets, &model, &names);
            }
        }
    });
    let close = close_on(&dialog);
    widgets.search.connect_stop_search(move |_| close());
    widgets.search.connect_activate({
        let list = widgets.list.clone();
        move |_| {
            if let Some(row) = list.selected_row() {
                row.activate();
            }
        }
    });
    let close = close_on(&dialog);
    let selected_position = Rc::new(std::cell::Cell::new(None));
    super::dialog_focus::restore_on_close(&dialog, view, {
        let selected_position = selected_position.clone();
        move |view| {
            if let Some(position) = selected_position.get() {
                select_and_scroll(view, position);
            }
        }
    });
    widgets.list.connect_row_activated(move |_, row| {
        let position = usize::try_from(row.index())
            .ok()
            .and_then(|index| positions.borrow().get(index).copied());
        selected_position.set(position);
        close();
    });
    dialog.set_child(Some(&widgets.content));
    dialog.present(Some(view));
    widgets.search.grab_focus();
}

fn select_and_scroll(view: &gtk::ColumnView, position: u32) {
    let Some(model) = view.model().filter(|model| position < model.n_items()) else {
        return;
    };
    model.select_item(position, true);
    view.scroll_to(
        position,
        None::<&gtk::ColumnViewColumn>,
        gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT,
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SerializationReached(std::cell::Cell<bool>);

    impl serde::Serialize for SerializationReached {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            self.0.set(true);
            serializer.serialize_str("later")
        }
    }

    #[derive(serde::Serialize)]
    struct JsonSearchSerializationProbe<'a> {
        first: &'a str,
        tail: &'a SerializationReached,
    }

    fn rows() -> Vec<Vec<Value>> {
        vec![
            vec![Value::Int(1), Value::Text("Alice Smith".into())],
            vec![Value::Int(2), Value::Null],
            vec![Value::Int(3), Value::Text("bob alice".into())],
            vec![Value::Int(4), Value::Text("carol".into())],
        ]
    }

    fn find(query: &str, limit: usize) -> Vec<(u32, usize)> {
        let rows = rows();
        matching_rows(
            rows.len() as u32,
            |p, visit| {
                if let Some(cells) = rows.get(p as usize) {
                    for (column, value) in cells.iter().enumerate() {
                        if visit(column, value) {
                            break;
                        }
                    }
                }
            },
            query,
            limit,
        )
        .into_iter()
        .map(|hit| (hit.position, hit.column))
        .collect()
    }

    #[test]
    fn a_match_is_case_insensitive_and_reports_the_first_matching_column() {
        assert_eq!(find("ALICE", 10), vec![(0, 1), (2, 1)]);
        assert_eq!(find("3", 10), vec![(2, 0)]);
    }

    #[test]
    fn case_insensitive_search_keeps_unicode_lowercase_expansions() {
        assert!(CaseInsensitivePattern::new("i\u{307}").matches("before İ after"));
    }

    #[test]
    fn typed_json_search_matches_its_compact_serialized_representation() {
        let json = serde_json::json!({"Name": "Zoë", "count": 42});
        let value = Value::Json(Box::new(json.clone()));
        let serialized = json.to_string();
        for query in ["name", "ZOË", "42", "missing"] {
            let pattern = CaseInsensitivePattern::new(query);
            assert_eq!(matches_value(&value, &pattern), pattern.matches(&serialized));
        }
    }

    #[test]
    fn typed_json_search_matches_unicode_split_across_writer_chunks() {
        let pattern = CaseInsensitivePattern::new("😀i\u{307}");
        let serialized = serde_json::json!({"emoji": "😀İ"}).to_string();
        let mut writer = JsonSearchWriter::new(&pattern);
        for byte in serialized.bytes() {
            if writer.write_all(&[byte]).is_err() {
                break;
            }
        }
        assert!(writer.matches());
    }

    #[test]
    fn typed_json_search_stops_serialization_after_a_match() {
        let pattern = CaseInsensitivePattern::new("needle");
        let mut writer = JsonSearchWriter::new(&pattern);
        let tail = SerializationReached(std::cell::Cell::new(false));
        let probe = JsonSearchSerializationProbe {
            first: "needle",
            tail: &tail,
        };
        let error = serde_json::to_writer(&mut writer, &probe).unwrap_err();
        assert!(error.is_io());
        assert!(writer.matches());
        assert!(!tail.0.get());
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        assert!(find("", 10).is_empty());
        assert!(find("   ", 10).is_empty());
    }

    #[test]
    fn a_missing_projected_cell_is_not_a_search_match() {
        let found = matching_rows(
            1,
            |_, visit| {
                visit(0, &Value::Int(1));
            },
            "not fetched",
            10,
        );
        assert!(found.is_empty());
    }

    #[test]
    fn null_binary_and_undecodable_values_keep_their_search_labels() {
        let cells = vec![
            Some(Value::Null),
            Some(Value::Bytes(vec![0xFF, 0xFE])),
            Some(Value::Undecodable("NUMERIC".into())),
        ];
        assert_eq!(
            first_matching_cell(&cells, &CaseInsensitivePattern::new("null"))
                .unwrap()
                .0,
            0
        );
        assert_eq!(
            first_matching_cell(&cells, &CaseInsensitivePattern::new("2 bytes"))
                .unwrap()
                .0,
            1
        );
        assert_eq!(
            first_matching_cell(&cells, &CaseInsensitivePattern::new("numeric"))
                .unwrap()
                .0,
            2
        );
    }

    #[test]
    fn a_long_cell_remains_searchable_beyond_its_visible_preview() {
        let text = format!("{}needle", "😀".repeat(10_001));
        assert!(!crate::ui::grid::display::value_to_display_text(&Value::Text(text.clone())).contains("needle"));
        let result = std::sync::Arc::new(tablepro_core::QueryResult {
            columns: Vec::new(),
            rows: vec![vec![Value::Int(1), Value::Text(text)]],
            truncated: false,
        });
        let store = crate::ui::row_store::RowStore::from_shared_with_previews(result, vec![0]);

        let found = matching_rows(
            1,
            |position, visit| {
                store.visit_search_cells(position, visit);
            },
            "needle",
            10,
        );
        assert_eq!(
            found.iter().map(|hit| (hit.position, hit.column)).collect::<Vec<_>>(),
            vec![(0, 1)]
        );
    }

    #[test]
    fn the_scan_stops_at_the_limit() {
        assert_eq!(find("a", 2), vec![(0, 1), (2, 1)]);
        assert_eq!(find("a", 1), vec![(0, 1)]);
    }

    #[test]
    fn a_long_cell_is_shown_as_one_short_line() {
        let shown = preview(&format!("first\nsecond {}", "x".repeat(200)));
        assert!(!shown.contains('\n'));
        assert_eq!(shown.chars().count(), PREVIEW_CHARS + 1);
        assert!(shown.ends_with('…'));
    }

    #[test]
    fn large_shared_value_search_has_bounded_temporary_memory() {
        const KIND_ENV: &str = "BOOKIE_LARGE_ROW_SEARCH_KIND";
        if let Ok(kind) = std::env::var(KIND_ENV) {
            let mut text = String::with_capacity(64 * 1024 * 1024);
            text.extend(std::iter::repeat_n('x', 64 * 1024 * 1024 - 6));
            text.push_str("needle");
            let value = match kind.as_str() {
                "text" => Value::Text(text),
                "json" => Value::Json(Box::new(serde_json::Value::String(text))),
                _ => panic!("unknown row search fixture kind"),
            };
            let result = std::sync::Arc::new(tablepro_core::QueryResult {
                columns: Vec::new(),
                rows: vec![vec![value]],
                truncated: false,
            });
            let store = crate::ui::row_store::RowStore::from_shared(result);
            let before = peak_memory_kib();
            let found = matching_rows(
                1,
                |position, visit| {
                    store.visit_search_cells(position, visit);
                },
                "needle",
                1,
            );
            let extra = peak_memory_kib().saturating_sub(before);
            eprintln!("row-search-memory kind={kind} extra_peak_kib={extra}");
            assert_eq!(found.len(), 1);
            assert!(
                extra < 16 * 1024,
                "large-value search allocated {extra} KiB beyond its source"
            );
            return;
        }

        for kind in ["text", "json"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "ui::grid::row_find::tests::large_shared_value_search_has_bounded_temporary_memory",
                    "--nocapture",
                ])
                .env(KIND_ENV, kind)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{kind} row search exceeded its memory budget:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    fn peak_memory_kib() -> usize {
        std::fs::read_to_string("/proc/self/status")
            .expect("process memory status")
            .lines()
            .find_map(|line| line.strip_prefix("VmHWM:")?.split_whitespace().next()?.parse().ok())
            .expect("peak resident memory")
    }
}
