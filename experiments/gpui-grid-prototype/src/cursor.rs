pub const COLUMNS: usize = 4;
pub const COLUMN_NAMES: [&str; COLUMNS] = ["Value", "Label", "State", "Typed"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub row: usize,
    pub column: usize,
}

impl Cursor {
    pub fn label(self) -> String {
        format!("Row {}, {}", self.row + 1, COLUMN_NAMES[self.column])
    }
}

pub fn clamp(cursor: Option<Cursor>, rows: usize) -> Option<Cursor> {
    if rows == 0 {
        return None;
    }
    let cursor = cursor.unwrap_or(Cursor { row: 0, column: 0 });
    Some(Cursor {
        row: cursor.row.min(rows - 1),
        column: cursor.column.min(COLUMNS - 1),
    })
}

pub fn move_cursor(cursor: Option<Cursor>, key: &str, rows: usize, page_rows: usize, ctrl: bool) -> Option<Cursor> {
    let start = clamp(cursor, rows)?;
    let step = page_rows.saturating_sub(1).max(1);
    let (row, column) = match key {
        "up" => (start.row.saturating_sub(1), start.column),
        "down" => (start.row + 1, start.column),
        "left" => (start.row, start.column.saturating_sub(1)),
        "right" => (start.row, start.column + 1),
        "pageup" => (start.row.saturating_sub(step), start.column),
        "pagedown" => (start.row + step, start.column),
        "home" if ctrl => (0, 0),
        "home" => (start.row, 0),
        "end" if ctrl => (rows - 1, COLUMNS - 1),
        "end" => (start.row, COLUMNS - 1),
        _ => return None,
    };
    clamp(Some(Cursor { row, column }), rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: fn(usize, usize) -> Option<Cursor> = |row, column| Some(Cursor { row, column });

    #[test]
    fn arrows_move_one_cell_and_stop_at_the_edges() {
        assert_eq!(move_cursor(AT(5, 1), "down", 100, 10, false), AT(6, 1));
        assert_eq!(move_cursor(AT(0, 0), "up", 100, 10, false), AT(0, 0));
        assert_eq!(move_cursor(AT(0, 3), "right", 100, 10, false), AT(0, 3));
        assert_eq!(move_cursor(AT(0, 0), "left", 100, 10, false), AT(0, 0));
    }

    #[test]
    fn page_keys_move_a_screenful_minus_one_and_clamp() {
        assert_eq!(move_cursor(AT(20, 2), "pagedown", 100, 10, false), AT(29, 2));
        assert_eq!(move_cursor(AT(3, 2), "pageup", 100, 10, false), AT(0, 2));
        assert_eq!(move_cursor(AT(95, 0), "pagedown", 100, 10, false), AT(99, 0));
    }

    #[test]
    fn home_and_end_go_to_the_row_ends_and_with_ctrl_to_the_corners() {
        assert_eq!(move_cursor(AT(7, 2), "home", 100, 10, false), AT(7, 0));
        assert_eq!(move_cursor(AT(7, 2), "end", 100, 10, false), AT(7, 3));
        assert_eq!(move_cursor(AT(7, 2), "home", 100, 10, true), AT(0, 0));
        assert_eq!(move_cursor(AT(7, 2), "end", 100, 10, true), AT(99, 3));
    }

    #[test]
    fn an_unknown_key_or_an_empty_grid_gives_no_move() {
        assert_eq!(move_cursor(AT(1, 1), "x", 100, 10, false), None);
        assert_eq!(move_cursor(None, "down", 0, 10, false), None);
    }

    #[test]
    fn the_first_move_from_no_cursor_starts_at_the_top_left() {
        assert_eq!(move_cursor(None, "down", 100, 10, false), AT(1, 0));
        assert_eq!(clamp(AT(500, 9), 100), AT(99, 3));
    }

    #[test]
    fn the_label_names_the_row_and_the_column() {
        assert_eq!(Cursor { row: 4, column: 2 }.label(), "Row 5, State");
    }
}
