pub const MAX_PASTE_CELLS: usize = 5_000;

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

pub fn parse_clipboard_grid(text: &str) -> Vec<Vec<Option<String>>> {
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
pub struct PasteCell {
    pub row: u32,
    pub col: usize,
    pub text: Option<String>,
}

pub fn paste_plan(
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

#[cfg(test)]
#[path = "clipboard_grid_tests.rs"]
mod tests;
