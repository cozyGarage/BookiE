use super::csv_import::ImportError;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextRows {
    text: String,
    short_ends: Vec<u16>,
    wide_ends: Vec<u32>,
    row_ends: Vec<RowEnd>,
    workbook_cells: Vec<WorkbookCell>,
    workbook: bool,
    width: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct RowEnd {
    bytes: usize,
    short_cells: usize,
    wide_cells: usize,
}

#[derive(Debug, Clone, Copy)]
enum CellEnds<'a> {
    Short(&'a [u16]),
    Wide(&'a [u32]),
}

impl CellEnds<'_> {
    fn get(self, index: usize) -> Option<usize> {
        match self {
            Self::Short(ends) => ends.get(index).copied().map(usize::from),
            Self::Wide(ends) => ends.get(index).map(|&end| end as usize),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkbookValue {
    Bool(bool),
    Int(i64),
    FloatBits(u64),
    DateTime(chrono::NaiveDateTime),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkbookCell {
    row: usize,
    column: usize,
    value: WorkbookValue,
}

#[derive(Debug, Clone, Copy)]
pub struct TextRow<'a> {
    text: &'a str,
    cell_ends: CellEnds<'a>,
    workbook_cells: &'a [WorkbookCell],
    workbook: bool,
    width: usize,
}

impl TextRows {
    pub(super) fn push<'a>(&mut self, fields: impl IntoIterator<Item = &'a str>) -> Result<(), ImportError> {
        let start = self.end();
        for field in fields {
            let end = (self.text.len() - start.bytes).saturating_add(field.len());
            let end = u32::try_from(end).map_err(|_| ImportError::FileTooLarge {
                size: end as u64,
                limit: u64::from(u32::MAX),
            })?;
            self.text.push_str(field);
            if self.wide_ends.len() == start.wide_cells {
                if let Ok(short) = u16::try_from(end) {
                    self.short_ends.push(short);
                    continue;
                }
                self.wide_ends
                    .extend(self.short_ends.drain(start.short_cells..).map(u32::from));
            }
            self.wide_ends.push(end);
        }
        let end = self.end();
        self.width = self
            .width
            .max(end.short_cells - start.short_cells + end.wide_cells - start.wide_cells);
        self.row_ends.push(end);
        Ok(())
    }

    pub(super) fn push_workbook_row<'a>(
        &mut self,
        fields: impl IntoIterator<Item = (&'a str, Option<WorkbookValue>)>,
    ) -> Result<(), ImportError> {
        self.workbook = true;
        let row = self.len();
        let mut cells = std::mem::take(&mut self.workbook_cells);
        let result = self.push(fields.into_iter().enumerate().map(|(column, (text, native))| {
            if let Some(value) = native {
                cells.push(WorkbookCell { row, column, value });
            }
            text
        }));
        self.workbook_cells = cells;
        result
    }

    fn end(&self) -> RowEnd {
        RowEnd {
            bytes: self.text.len(),
            short_cells: self.short_ends.len(),
            wide_cells: self.wide_ends.len(),
        }
    }

    pub(super) fn width(&self) -> usize {
        self.width
    }

    pub(super) fn widen_to(&mut self, width: usize) {
        self.width = self.width.max(width);
    }

    pub fn len(&self) -> usize {
        self.row_ends.len()
    }

    pub fn is_empty(&self) -> bool {
        self.row_ends.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<TextRow<'_>> {
        self.row_ends.get(index).map(|&end| self.row(index, end))
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = TextRow<'_>> + '_ {
        self.row_ends
            .iter()
            .enumerate()
            .map(|(index, &end)| self.row(index, end))
    }

    fn row(&self, index: usize, end: RowEnd) -> TextRow<'_> {
        let start = index
            .checked_sub(1)
            .map_or_else(RowEnd::default, |prev| self.row_ends[prev]);
        let cell_ends = if end.wide_cells == start.wide_cells {
            CellEnds::Short(&self.short_ends[start.short_cells..end.short_cells])
        } else {
            CellEnds::Wide(&self.wide_ends[start.wide_cells..end.wide_cells])
        };
        TextRow {
            text: &self.text[start.bytes..end.bytes],
            cell_ends,
            workbook_cells: &self.workbook_cells[self.workbook_cells.partition_point(|cell| cell.row < index)
                ..self.workbook_cells.partition_point(|cell| cell.row <= index)],
            workbook: self.workbook,
            width: self.width,
        }
    }

    pub fn truncate(&mut self, len: usize) {
        if len >= self.len() {
            return;
        }
        self.row_ends.truncate(len);
        self.workbook_cells
            .truncate(self.workbook_cells.partition_point(|cell| cell.row < len));
        let end = self.row_ends.last().copied().unwrap_or_default();
        self.short_ends.truncate(end.short_cells);
        self.wide_ends.truncate(end.wide_cells);
        self.text.truncate(end.bytes);
    }
}

impl<'a> TextRow<'a> {
    pub(super) fn is_workbook(self) -> bool {
        self.workbook
    }
    pub fn len(self) -> usize {
        self.width
    }

    pub fn is_empty(self) -> bool {
        self.width == 0
    }

    pub fn get(self, index: usize) -> Option<&'a str> {
        if index >= self.width {
            return None;
        }
        let Some(end) = self.cell_ends.get(index) else {
            return Some("");
        };
        let start = index
            .checked_sub(1)
            .and_then(|prev| self.cell_ends.get(prev))
            .unwrap_or(0);
        self.text.get(start..end)
    }

    pub(super) fn workbook_value(self, index: usize) -> Option<WorkbookValue> {
        let index = self
            .workbook_cells
            .binary_search_by_key(&index, |cell| cell.column)
            .ok()?;
        Some(self.workbook_cells[index].value)
    }

    pub fn iter(self) -> impl ExactSizeIterator<Item = &'a str> {
        (0..self.width).map(move |index| self.get(index).unwrap_or_default())
    }
}

#[cfg(test)]
impl TextRows {
    pub(crate) fn from_test_rows<'a>(rows: impl IntoIterator<Item = impl IntoIterator<Item = &'a str>>) -> Self {
        let mut result = Self::default();
        for row in rows {
            result.push(row).unwrap();
        }
        result
    }

    pub(crate) fn as_test_rows(&self) -> Vec<Vec<&str>> {
        self.iter().map(|row| row.iter().collect()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_row_preserves_fields_on_both_sides_of_the_offset_boundary() {
        let large = "é".repeat(32_768);
        let mut rows = TextRows::default();
        rows.push(["a", "b"]).unwrap();
        rows.push(["pre", &large, "post"]).unwrap();
        rows.push(["next"]).unwrap();
        assert_eq!(rows.get(0).unwrap().iter().collect::<Vec<_>>(), ["a", "b", ""]);
        assert_eq!(rows.get(1).unwrap().iter().collect::<Vec<_>>(), ["pre", &large, "post"]);
        assert_eq!(rows.get(2).unwrap().iter().collect::<Vec<_>>(), ["next", "", ""]);

        rows.truncate(1);
        rows.push(["reused"]).unwrap();
        assert_eq!(rows.get(1).unwrap().get(0), Some("reused"));
        assert!(rows.wide_ends.is_empty());
    }

    #[test]
    fn sparse_rows_pad_without_allocating_cells_and_keep_unicode_offsets() {
        let mut rows = TextRows::default();
        rows.push([""]).unwrap();
        rows.push(["東京", "é", ""]).unwrap();
        rows.push(["last"]).unwrap();

        assert_eq!(rows.short_ends.len(), 5);
        assert_eq!(rows.text, "東京élast");
        assert_eq!(rows.get(0).unwrap().iter().collect::<Vec<_>>(), ["", "", ""]);
        assert_eq!(rows.get(1).unwrap().iter().collect::<Vec<_>>(), ["東京", "é", ""]);
        assert_eq!(rows.get(2).unwrap().get(2), Some(""));
        assert_eq!(rows.get(2).unwrap().get(3), None);

        rows.truncate(2);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows.text, "東京é");
        assert_eq!(rows.get(2).map(TextRow::len), None);
    }
}
