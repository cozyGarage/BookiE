use relm4::ComponentSender;
use relm4::gtk::{self, gdk, glib};

use super::value_parse::parse_pasted_cell_with_style;
use super::*;

const MAX_PASTE_CELLS: usize = 5_000;
const MAX_PASTE_BYTES: usize = tablepro_core::import::MAX_FIELD_BYTES;

fn paste_size_allowed(size: usize) -> bool {
    size <= MAX_PASTE_BYTES
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) struct PasteSnapshot {
    page_request_id: Option<uuid::Uuid>,
    row_keys: Vec<Option<crate::services::change_tracker::RowKey>>,
}

#[derive(Debug)]
pub(in crate::ui) struct PasteAnchor {
    pub(in crate::ui) row: u32,
    pub(in crate::ui) col: usize,
    pub(in crate::ui) row_key: Vec<Value>,
    pub(in crate::ui) displayed: Option<Vec<usize>>,
}

#[derive(Debug)]
pub(in crate::ui) struct PasteTextInput {
    pub(in crate::ui) row: u32,
    pub(in crate::ui) col: usize,
    pub(in crate::ui) row_key: Vec<Value>,
    pub(in crate::ui) text: String,
    pub(in crate::ui) displayed: Option<Vec<usize>>,
    pub(in crate::ui) snapshot: PasteSnapshot,
}

impl PasteSnapshot {
    fn new(
        page_request_id: Option<uuid::Uuid>,
        row_keys: Vec<Option<crate::services::change_tracker::RowKey>>,
    ) -> Self {
        Self {
            page_request_id,
            row_keys,
        }
    }

    fn matches(
        &self,
        current_page_request_id: Option<uuid::Uuid>,
        target_rows: &[u32],
        mut row_key_at: impl FnMut(u32) -> Option<crate::services::change_tracker::RowKey>,
    ) -> bool {
        if self.page_request_id != current_page_request_id {
            return false;
        }
        target_rows.iter().all(|row| {
            let Some(expected) = self.row_keys.get(*row as usize).and_then(Option::as_ref) else {
                return false;
            };
            row_key_at(*row).as_ref() == Some(expected)
        })
    }
}

fn is_spreadsheet_quoted_cell(after_open: &[char]) -> bool {
    let mut needs_quoting = false;
    let mut index = 0;
    while let Some(&c) = after_open.get(index) {
        index += 1;
        match c {
            '"' if after_open.get(index) == Some(&'"') => {
                needs_quoting = true;
                index += 1;
            }
            '"' => {
                let ends_cell = matches!(after_open.get(index), None | Some('\t' | '\n' | '\r'));
                return ends_cell && needs_quoting;
            }
            '\t' | '\n' | '\r' => needs_quoting = true,
            _ => {}
        }
    }
    false
}

fn undo_copy_markers(cell: String) -> Option<String> {
    let is_marker = |text: &str| {
        text.strip_prefix("\\N")
            .is_some_and(|rest| rest.bytes().all(|b| b == b'N'))
    };
    if is_marker(&cell) {
        return None;
    }
    let inner = cell.strip_prefix('"').and_then(|rest| rest.strip_suffix('"'));
    if let Some(inner) = inner.filter(|inner| inner.is_empty() || is_marker(inner)) {
        return Some(inner.to_string());
    }
    let guarded = inner
        .and_then(|inner| inner.strip_prefix('\''))
        .filter(|inner| inner.starts_with(['=', '+', '-', '@', '\t', '\r']) && !inner.contains('"'));
    Some(guarded.map_or(cell.clone(), str::to_string))
}

pub(super) fn parse_clipboard_grid(text: &str) -> Vec<Vec<Option<String>>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut was_quoted = false;
    let finish = |cell: String, was_quoted: bool| {
        if was_quoted {
            Some(cell)
        } else {
            undo_copy_markers(cell)
        }
    };
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while let Some(&c) = chars.get(index) {
        index += 1;
        match c {
            '"' if quoted && chars.get(index) == Some(&'"') => {
                cell.push('"');
                index += 1;
            }
            '"' if quoted => quoted = false,
            '"' if cell.is_empty() && !was_quoted && is_spreadsheet_quoted_cell(&chars[index..]) => {
                quoted = true;
                was_quoted = true;
            }
            '\t' if !quoted => {
                row.push(finish(std::mem::take(&mut cell), was_quoted));
                was_quoted = false;
            }
            '\r' if !quoted && chars.get(index) == Some(&'\n') => {}
            '\n' if !quoted => {
                row.push(finish(std::mem::take(&mut cell), was_quoted));
                rows.push(std::mem::take(&mut row));
                was_quoted = false;
            }
            _ => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() || was_quoted {
        row.push(finish(cell, was_quoted));
        rows.push(row);
    }
    rows
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct PasteCell {
    pub(super) row: u32,
    pub(super) col: usize,
    pub(super) text: Option<String>,
}

pub(super) fn paste_plan(
    anchor_row: u32,
    anchor_col: usize,
    grid: &[Vec<Option<String>>],
    loaded_rows: u32,
    displayed: &[usize],
) -> Result<Vec<PasteCell>, String> {
    let width = grid.iter().map(Vec::len).max().unwrap_or(0);
    let height = grid.len();
    if height == 0 || width == 0 {
        return Err(crate::tr!("The clipboard has no text to paste"));
    }
    if height.saturating_mul(width) > MAX_PASTE_CELLS {
        return Err(crate::tr!("That is too many cells to paste at once"));
    }
    if (anchor_row as usize).saturating_add(height) > loaded_rows as usize {
        return Err(crate::tr!("The pasted rows run past the last loaded row"));
    }
    let Some(start) = displayed.iter().position(|col| *col == anchor_col) else {
        return Err(crate::tr!("The column you clicked is hidden, so nothing was pasted"));
    };
    if start.saturating_add(width) > displayed.len() {
        return Err(crate::tr!("The pasted columns run past the last column"));
    }
    let mut cells = Vec::with_capacity(height * width);
    for (r, line) in grid.iter().enumerate() {
        for (c, text) in line.iter().enumerate() {
            cells.push(PasteCell {
                row: anchor_row + r as u32,
                col: displayed[start + c],
                text: text.clone(),
            });
        }
    }
    Ok(cells)
}

pub(super) fn paste_shortcut(sender: ComponentSender<BrowseTab>) -> gtk::Shortcut {
    gtk::Shortcut::builder()
        .trigger(&crate::ui::shortcut::parse("<Primary>v"))
        .action(&gtk::CallbackAction::new(move |widget, _| {
            match crate::ui::grid::focused_cell_identity(widget).or_else(|| crate::ui::grid::last_clicked_cell(widget))
            {
                Some((row, col, row_key)) => sender.input(BrowseTabInput::Paste(PasteAnchor {
                    row,
                    col,
                    row_key,
                    displayed: crate::ui::grid::displayed_column_order(widget),
                })),
                None => sender.input(BrowseTabInput::PasteNotSupported),
            }
            glib::Propagation::Stop
        }))
        .build()
}

impl BrowseTab {
    pub(super) fn handle_paste(&mut self, anchor: PasteAnchor, sender: ComponentSender<Self>) {
        let PasteAnchor {
            row,
            col,
            row_key,
            displayed,
        } = anchor;
        if !self.row_position_still_holds(row, &row_key) {
            let _ = sender.output(BrowseTabOutput::ShowToast(crate::tr!(
                "The rows changed since you clicked the cell, so nothing was pasted"
            )));
            return;
        }
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let clipboard = display.clipboard();
        let snapshot = self.capture_paste_snapshot();
        glib::spawn_future_local(async move {
            if let Ok(Some(text)) = clipboard.read_text_future().await {
                if !paste_size_allowed(text.len()) {
                    let _ = sender.output(BrowseTabOutput::ShowToast(crate::tr!(
                        "Clipboard text is too large to paste; nothing was pasted"
                    )));
                    return;
                }
                sender.input(BrowseTabInput::PasteText(PasteTextInput {
                    row,
                    col,
                    row_key,
                    text: text.to_string(),
                    displayed,
                    snapshot,
                }));
            }
        });
    }

    fn check_paste_cell(&self, cell: &PasteCell) -> Result<Value, String> {
        let column = self.current_columns.get(cell.col);
        let row = self.row_object_at(cell.row);
        let allowed = column
            .zip(row.as_ref())
            .is_some_and(|(column, row)| crate::ui::grid::cell_allows_inline_edit(column, &row.cell_value(cell.col)));
        let name = column.map(|column| column.name.as_str()).unwrap_or_default();
        if !allowed {
            return Err(
                crate::tr!("{column} on row {row} can't be edited, so nothing was pasted")
                    .replace("{column}", name)
                    .replace("{row}", &(cell.row + 1).to_string()),
            );
        }
        let current = row.as_ref().map(|row| row.cell_value(cell.col));
        parse_pasted_cell_with_style(
            cell.text.as_deref(),
            column,
            &self.driver_id,
            self.enum_input_style,
            current.as_ref(),
        )
        .map_err(|message| {
            crate::tr!("{column} on row {row}: {message}. Nothing was pasted.")
                .replace("{column}", name)
                .replace("{row}", &(cell.row + 1).to_string())
                .replace("{message}", &message)
        })
    }

    pub(super) fn handle_paste_text(&mut self, input: PasteTextInput, sender: ComponentSender<Self>) {
        let PasteTextInput {
            row,
            col,
            row_key,
            text,
            displayed,
            snapshot,
        } = input;
        if snapshot.page_request_id != self.page_requests.current_id() || !self.row_position_still_holds(row, &row_key)
        {
            let _ = sender.output(BrowseTabOutput::ShowToast(crate::tr!(
                "The rows changed since you clicked the cell, so nothing was pasted"
            )));
            return;
        }
        let loaded = self.row_count_loaded();
        let displayed = displayed.unwrap_or_else(|| (0..self.current_columns.len()).collect());
        let cells = match paste_plan(row, col, &parse_clipboard_grid(&text), loaded, &displayed) {
            Ok(cells) => cells,
            Err(message) => {
                let _ = sender.output(BrowseTabOutput::ShowToast(message));
                return;
            }
        };
        let target_rows: Vec<u32> = cells.iter().map(|cell| cell.row).collect();
        if !snapshot.matches(self.page_requests.current_id(), &target_rows, |position| {
            self.paste_row_key_at(position)
        }) {
            let _ = sender.output(BrowseTabOutput::ShowToast(crate::tr!(
                "That row moved while you were editing. The change was not applied."
            )));
            return;
        }
        let values = match cells
            .iter()
            .map(|cell| self.check_paste_cell(cell))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(values) => values,
            Err(message) => {
                let _ = sender.output(BrowseTabOutput::ShowToast(message));
                return;
            }
        };
        for (cell, value) in cells.iter().zip(values) {
            self.stage_grid_cell_value(cell.row, cell.col, value, &sender);
        }
    }

    pub(super) fn capture_paste_snapshot(&self) -> PasteSnapshot {
        let store = self.row_store();
        let pk_indices = self.pk_indices();
        let row_keys = (0..self.row_count_loaded())
            .map(|position| store.as_ref()?.row_key_at(position, &pk_indices))
            .collect();
        PasteSnapshot::new(self.page_requests.current_id(), row_keys)
    }

    fn paste_row_key_at(&self, position: u32) -> Option<crate::services::change_tracker::RowKey> {
        self.row_store()?.row_key_at(position, &self.pk_indices())
    }

    fn row_count_loaded(&self) -> u32 {
        self.current_selection
            .as_ref()
            .map_or(0, |selection| selection.n_items())
    }
}

#[cfg(test)]
#[path = "paste_tests.rs"]
mod tests;

#[cfg(test)]
mod roundtrip_tests {
    use super::parse_clipboard_grid as parse_with_nulls;

    fn parse_clipboard_grid(text: &str) -> Vec<Vec<String>> {
        parse_with_nulls(text)
            .into_iter()
            .map(|row| row.into_iter().map(Option::unwrap).collect())
            .collect()
    }

    fn spreadsheet_encode(rows: &[Vec<String>]) -> String {
        rows.iter()
            .map(|row| {
                row.iter()
                    .map(|cell| {
                        if cell.contains(['\t', '\n', '\r', '"']) {
                            format!("\"{}\"", cell.replace('"', "\"\""))
                        } else {
                            cell.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn strings(alphabet: &[char], max_len: usize) -> Vec<String> {
        let mut all = vec![String::new()];
        let mut frontier = vec![String::new()];
        for _ in 0..max_len {
            frontier = frontier
                .iter()
                .flat_map(|prefix| alphabet.iter().map(move |c| format!("{prefix}{c}")))
                .collect();
            all.extend(frontier.iter().cloned());
        }
        all
    }

    #[test]
    fn whatever_a_spreadsheet_copies_is_pasted_back_unchanged() {
        let cells = strings(&['a', '"', '\t', '\n', ' '], 3);
        let mut failures = Vec::new();
        for first in &cells {
            for second in &cells {
                let grid = vec![vec![first.clone(), second.clone()]];
                let text = spreadsheet_encode(&grid);
                if text.is_empty() {
                    continue;
                }
                let parsed = parse_clipboard_grid(&text);
                if parsed != grid {
                    failures.push((grid, text, parsed));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} copies changed; first: {:?}",
            failures.len(),
            cells.len() * cells.len(),
            failures.first()
        );
    }
    #[test]
    fn carriage_returns_and_several_rows_survive_the_same_round_trip() {
        let with_returns = strings(&['a', '"', '\t', '\n', '\r', ' '], 2);
        for first in &with_returns {
            for second in &with_returns {
                let grid = vec![vec![first.clone(), second.clone()]];
                let text = spreadsheet_encode(&grid);
                if !text.is_empty() {
                    assert_eq!(parse_clipboard_grid(&text), grid, "{text:?}");
                }
            }
        }
        let small = strings(&['a', '"', '\t', '\n', '\r', ' '], 1);
        for a in &small {
            for b in &small {
                for c in &small {
                    for d in &small {
                        let grid = vec![vec![a.clone(), b.clone()], vec![c.clone(), d.clone()]];
                        let text = spreadsheet_encode(&grid);
                        assert_eq!(parse_clipboard_grid(&text), grid, "{text:?}");
                    }
                }
            }
        }
    }
}
