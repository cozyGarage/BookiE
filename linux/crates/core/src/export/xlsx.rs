use std::io::Write;

use rust_decimal::prelude::ToPrimitive;
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
        let target = u32::try_from(index + 1).map_err(|_| ExportError::WorkbookTooLarge {
            limit: MAX_WORKBOOK_ROWS,
            rows: index + 1,
        })?;
        for (column, value) in row.iter().enumerate() {
            write_cell(&mut self.sheet, target, cell_column(column)?, value, &self.formats)?;
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
    u16::try_from(index).map_err(|_| ExportError::WorkbookColumnLimit { columns: index + 1 })
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
        Value::Int(value) => sheet.write_number(row, column, *value as f64).map(drop),
        Value::Float(value) if value.is_finite() => sheet.write_number(row, column, *value).map(drop),
        Value::Decimal(value) => match value.to_f64().filter(|number| number.is_finite()) {
            Some(number) => sheet.write_number(row, column, number).map(drop),
            None => sheet.write_string(row, column, value.to_string()).map(drop),
        },
        Value::Date(value) => sheet.write_with_format(row, column, value, &formats.date).map(drop),
        Value::Time(value) => sheet.write_with_format(row, column, value, &formats.time).map(drop),
        Value::DateTime(value) => sheet
            .write_with_format(row, column, value, &formats.date_time)
            .map(drop),
        Value::TimestampTz(value) => sheet
            .write_with_format(row, column, &value.naive_utc(), &formats.date_time)
            .map(drop),
        other => match value_to_text(other) {
            Some(text) => sheet.write_string(row, column, text).map(drop),
            None => Ok(()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::test_support::column;

    #[test]
    fn a_result_over_the_sheet_limit_is_refused_with_the_limit_in_the_message() {
        let Err(error) = XlsxWriter::new(MAX_WORKBOOK_ROWS + 1) else {
            panic!("a result over the limit must be refused");
        };
        assert!(
            matches!(
                error,
                ExportError::WorkbookTooLarge {
                    limit: MAX_WORKBOOK_ROWS,
                    rows
                } if rows == MAX_WORKBOOK_ROWS + 1
            ),
            "{error:?}"
        );
        assert!(error.to_string().contains("1048575"), "{error}");
    }

    #[test]
    fn a_result_at_the_sheet_limit_is_accepted() {
        assert!(XlsxWriter::new(MAX_WORKBOOK_ROWS).is_ok(), "the limit itself must fit");
    }

    #[test]
    fn a_workbook_is_written_as_a_zip_container_with_one_sheet() {
        let Ok(mut writer) = XlsxWriter::new(1) else {
            panic!("one row fits");
        };
        let mut output: Vec<u8> = Vec::new();
        writer.begin(&mut output, &[column("id"), column("when")]).unwrap();
        writer
            .write_row(
                &mut output,
                0,
                &[Value::Int(7), Value::Date("2026-09-19".parse().unwrap())],
            )
            .unwrap();
        writer.finish(&mut output).unwrap();

        assert_eq!(&output[..2], b"PK");
        assert!(output.len() > 1000, "{}", output.len());
    }
}
