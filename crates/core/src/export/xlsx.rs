use std::io::Write;

use chrono::{Datelike, Timelike};
use rust_xlsxwriter::{Format, Workbook, Worksheet, XlsxError};

use super::error::ExportError;
use super::file::ResultWriter;
use super::value_to_text;
use crate::query::{ColumnInfo, Value};

/// A worksheet holds 1,048,576 rows including the header, so one fewer
/// data row fits than the sheet limit suggests.
pub const MAX_WORKBOOK_ROWS: usize = 1_048_575;

struct CellFormats {
    date: Format,
    time: Format,
    date_time: Format,
}

impl CellFormats {
    fn new() -> Self {
        Self {
            date: Format::new().set_num_format("yyyy\\-mm\\-dd"),
            time: Format::new().set_num_format("hh:mm:ss"),
            date_time: Format::new().set_num_format("yyyy\\-mm\\-dd hh:mm:ss"),
        }
    }
}

pub(crate) struct XlsxWriter {
    sheet: Worksheet,
    formats: CellFormats,
}

impl XlsxWriter {
    pub(crate) fn new(rows: usize) -> Result<Self, ExportError> {
        if rows > MAX_WORKBOOK_ROWS {
            return Err(ExportError::WorkbookTooLarge {
                limit: MAX_WORKBOOK_ROWS,
                rows,
            });
        }
        Ok(Self {
            sheet: Worksheet::new(),
            formats: CellFormats::new(),
        })
    }
}

impl ResultWriter for XlsxWriter {
    fn begin(&mut self, _output: &mut dyn Write, columns: &[ColumnInfo]) -> Result<(), ExportError> {
        let header = Format::new().set_bold();
        for (index, info) in columns.iter().enumerate() {
            self.sheet
                .write_string_with_format(0, cell_column(index)?, &info.name, &header)?;
        }
        Ok(())
    }

    fn write_row(&mut self, _output: &mut dyn Write, index: usize, row: &[Value]) -> Result<(), ExportError> {
        if index >= MAX_WORKBOOK_ROWS {
            return Err(ExportError::WorkbookTooLarge {
                limit: MAX_WORKBOOK_ROWS,
                rows: index.saturating_add(1),
            });
        }
        let target = (index + 1) as u32;
        for (column, value) in row.iter().enumerate() {
            let cell_column = cell_column(column)?;
            if matches!(value, Value::Text(text) if text.is_empty()) {
                return Err(ExportError::WorkbookEmptyText {
                    row: index + 1,
                    column: column + 1,
                });
            }
            write_cell(&mut self.sheet, target, cell_column, value, &self.formats)?;
        }
        Ok(())
    }

    fn finish(&mut self, output: &mut dyn Write) -> Result<(), ExportError> {
        let mut workbook = Workbook::new();
        workbook.push_worksheet(std::mem::replace(&mut self.sheet, Worksheet::new()));
        let bytes = workbook.save_to_buffer()?;
        output.write_all(&bytes)?;
        Ok(())
    }
}

fn cell_column(index: usize) -> Result<u16, ExportError> {
    if index >= 16_384 {
        return Err(ExportError::WorkbookColumnLimit {
            columns: index.saturating_add(1),
        });
    }
    Ok(index as u16)
}

fn write_cell(
    sheet: &mut Worksheet,
    row: u32,
    column: u16,
    value: &Value,
    formats: &CellFormats,
) -> Result<(), XlsxError> {
    match value {
        Value::Null => Ok(()),
        Value::Bool(value) => sheet.write_boolean(row, column, *value).map(drop),
        Value::Int(value) if value.unsigned_abs() <= 999_999_999_999_999 => {
            sheet.write_number(row, column, *value as f64).map(drop)
        }
        Value::Int(value) => sheet.write_string(row, column, value.to_string()).map(drop),
        Value::Float(value) if excel_safe_float_number(*value) => sheet.write_number(row, column, *value).map(drop),
        Value::Decimal(value) => sheet.write_string(row, column, value.to_string()).map(drop),
        Value::Date(value) if (1900..=9999).contains(&value.year()) => {
            sheet.write_with_format(row, column, value, &formats.date).map(drop)
        }
        Value::Time(value) if value.nanosecond() == 0 => {
            sheet.write_with_format(row, column, value, &formats.time).map(drop)
        }
        Value::DateTime(value) if (1900..=9999).contains(&value.year()) && value.nanosecond() == 0 => sheet
            .write_with_format(row, column, value, &formats.date_time)
            .map(drop),
        other => match value_to_text(other) {
            Some(text) => sheet.write_string(row, column, text).map(drop),
            None => Ok(()),
        },
    }
}

fn excel_safe_float_number(value: f64) -> bool {
    if !value.is_finite() {
        return false;
    }
    if value == 0.0 {
        return !value.is_sign_negative();
    }
    let magnitude = value.abs();
    if !(2.2251e-308..=9.99999999999999e307).contains(&magnitude) {
        return false;
    }
    let decimal = value.to_string();
    let digits: Vec<_> = decimal
        .split(['e', 'E'])
        .next()
        .unwrap_or(&decimal)
        .bytes()
        .filter(u8::is_ascii_digit)
        .collect();
    let Some(first) = digits.iter().position(|digit| *digit != b'0') else {
        return true;
    };
    let last = digits.iter().rposition(|digit| *digit != b'0').unwrap_or(first);
    last - first < 15
}

#[cfg(test)]
#[path = "xlsx_tests.rs"]
mod tests;
