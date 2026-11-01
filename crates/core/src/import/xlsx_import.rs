use std::borrow::Cow;
use std::io::{Cursor, Read};

use calamine::{DataRef, Reader, Xlsx, open_workbook_from_rs};
use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::reader::Reader as XmlReader;
use zip::ZipArchive;

use super::csv_import::{ImportError, MAX_COLUMNS, MAX_FIELD_BYTES, MAX_FILE_BYTES, MAX_IMPORT_ROWS};

const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
const MAX_SHARED_STRINGS_BYTES: u64 = MAX_FILE_BYTES;

pub fn is_xlsx(bytes: &[u8]) -> bool {
    bytes.starts_with(ZIP_MAGIC)
}

pub fn xlsx_to_csv(bytes: &[u8]) -> Result<Vec<u8>, ImportError> {
    check_output_size(0, bytes.len())?;
    check_workbook_xml_bounds(bytes)?;
    let mut workbook: Xlsx<_> = open_workbook_from_rs(Cursor::new(bytes)).map_err(|_| ImportError::NotSpreadsheet)?;
    for name in workbook.sheet_names() {
        let mut cells = workbook
            .worksheet_cells_reader(&name)
            .map_err(|_| ImportError::NotSpreadsheet)?;
        let mut writer = csv::WriterBuilder::new().flexible(true).from_writer(Vec::new());
        let mut row: Vec<String> = Vec::new();
        let mut row_payload_bytes = 0usize;
        let mut output_bytes = 0u64;
        let mut current = None;
        let (mut first_row, mut rows_written) = (0, 0usize);
        while let Some(cell) = cells.next_cell().map_err(|_| ImportError::NotSpreadsheet)? {
            let (row_index, column) = cell.get_position();
            let current_row = *current.get_or_insert_with(|| {
                first_row = row_index;
                row_index
            });
            check_row_order(current_row, row_index)?;
            if row_index != current_row {
                write_row(
                    &mut writer,
                    &mut row,
                    &mut rows_written,
                    &mut output_bytes,
                    &mut row_payload_bytes,
                )?;
                for _ in current_row + 1..row_index {
                    write_row(
                        &mut writer,
                        &mut row,
                        &mut rows_written,
                        &mut output_bytes,
                        &mut row_payload_bytes,
                    )?;
                }
                current = Some(row_index);
            }
            set_cell(
                &mut row,
                column as usize,
                cell.get_value(),
                (row_index - first_row) as usize + 1,
                output_bytes,
                &mut row_payload_bytes,
            )?;
        }
        if current.is_some() {
            write_row(
                &mut writer,
                &mut row,
                &mut rows_written,
                &mut output_bytes,
                &mut row_payload_bytes,
            )?;
            return writer.into_inner().map_err(|_| ImportError::NotSpreadsheet);
        }
    }
    Err(ImportError::NoColumns)
}

fn check_row_order(current_row: u32, next_row: u32) -> Result<(), ImportError> {
    if next_row < current_row {
        return Err(ImportError::NotSpreadsheet);
    }
    Ok(())
}

fn set_cell(
    row: &mut Vec<String>,
    column: usize,
    value: &DataRef<'_>,
    line: usize,
    output_bytes: u64,
    row_payload_bytes: &mut usize,
) -> Result<(), ImportError> {
    if column >= MAX_COLUMNS {
        return Err(ImportError::TooManyColumns {
            columns: column + 1,
            limit: MAX_COLUMNS,
        });
    }
    if row.len() <= column {
        row.resize(column + 1, String::new());
    }
    let text = cell_text(value);
    if text.len() > MAX_FIELD_BYTES {
        return Err(ImportError::FieldTooLong {
            line,
            limit: MAX_FIELD_BYTES,
        });
    }
    *row_payload_bytes = *row_payload_bytes - field_csv_bytes(&row[column]) + field_csv_bytes(&text);
    check_output_size(output_bytes, row_csv_bytes(*row_payload_bytes, row.len()))?;
    row[column] = text.into_owned();
    Ok(())
}

fn write_row(
    writer: &mut csv::Writer<Vec<u8>>,
    row: &mut Vec<String>,
    rows_written: &mut usize,
    output_bytes: &mut u64,
    row_payload_bytes: &mut usize,
) -> Result<(), ImportError> {
    if *rows_written > MAX_IMPORT_ROWS {
        return Err(ImportError::TooManyRows { limit: MAX_IMPORT_ROWS });
    }
    let row_bytes = row_csv_bytes(*row_payload_bytes, row.len());
    check_output_size(*output_bytes, row_bytes)?;
    writer
        .write_record(row.iter())
        .map_err(|_| ImportError::NotSpreadsheet)?;
    *output_bytes += row_bytes as u64;
    *row_payload_bytes = 0;
    row.clear();
    *rows_written += 1;
    Ok(())
}

fn check_workbook_xml_bounds(bytes: &[u8]) -> Result<(), ImportError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| ImportError::NotSpreadsheet)?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|_| ImportError::NotSpreadsheet)?;
        let name = file
            .name()
            .map_err(|_| ImportError::NotSpreadsheet)?
            .replace('\\', "/")
            .to_ascii_lowercase();
        if !name.ends_with(".xml") && !name.ends_with(".rels") {
            continue;
        }
        if file.size() > MAX_SHARED_STRINGS_BYTES {
            return Err(ImportError::FileTooLarge {
                size: file.size(),
                limit: MAX_SHARED_STRINGS_BYTES,
            });
        }
        if name.rsplit('/').next() == Some("sharedstrings.xml") {
            let mut xml = Vec::with_capacity(file.size() as usize);
            file.by_ref()
                .take(MAX_SHARED_STRINGS_BYTES + 1)
                .read_to_end(&mut xml)
                .map_err(|_| ImportError::NotSpreadsheet)?;
            if xml.len() as u64 > MAX_SHARED_STRINGS_BYTES {
                return Err(ImportError::FileTooLarge {
                    size: xml.len() as u64,
                    limit: MAX_SHARED_STRINGS_BYTES,
                });
            }
            check_shared_string_count(&xml)?;
        }
    }
    Ok(())
}

fn check_shared_string_count(xml: &[u8]) -> Result<(), ImportError> {
    let mut reader = XmlReader::from_reader(xml);
    let mut strings = 0usize;
    loop {
        match reader.read_event().map_err(|_| ImportError::NotSpreadsheet)? {
            Event::Start(element) | Event::Empty(element) => match element.local_name().as_ref() {
                "sst" => {
                    for attribute in element.attributes() {
                        let attribute = attribute.map_err(|_| ImportError::NotSpreadsheet)?;
                        if attribute.key.local_name().as_ref() == "uniqueCount" {
                            let count = attribute
                                .normalized_value(XmlVersion::Implicit1_0)
                                .map_err(|_| ImportError::NotSpreadsheet)?
                                .parse::<usize>()
                                .map_err(|_| ImportError::NotSpreadsheet)?;
                            check_shared_string_limit(count)?;
                        }
                    }
                }
                "si" => {
                    strings += 1;
                    check_shared_string_limit(strings)?;
                }
                _ => {}
            },
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

fn check_shared_string_limit(count: usize) -> Result<(), ImportError> {
    if count > MAX_IMPORT_ROWS {
        return Err(ImportError::TooManySharedStrings { limit: MAX_IMPORT_ROWS });
    }
    Ok(())
}

fn field_csv_bytes(value: &str) -> usize {
    let bytes = value.as_bytes();
    let quotes = bytes.iter().filter(|byte| **byte == b'"').count();
    if bytes.iter().any(|byte| matches!(byte, b',' | b'"' | b'\r' | b'\n')) {
        bytes.len() + quotes + 2
    } else {
        bytes.len()
    }
}

fn check_output_size(current: u64, added: usize) -> Result<(), ImportError> {
    let size = current.saturating_add(added as u64);
    if size > MAX_FILE_BYTES {
        return Err(ImportError::FileTooLarge {
            size,
            limit: MAX_FILE_BYTES,
        });
    }
    Ok(())
}

fn row_csv_bytes(payload: usize, columns: usize) -> usize {
    payload.saturating_add(if columns <= 1 && payload == 0 {
        3
    } else {
        columns.max(1)
    })
}

fn cell_text<'a>(value: &'a DataRef<'_>) -> Cow<'a, str> {
    match value {
        DataRef::Empty | DataRef::Error(_) => Cow::Borrowed(""),
        DataRef::Int(number) => Cow::Owned(number.to_string()),
        DataRef::Float(number) if number.fract() == 0.0 && number.abs() < 1e15 => Cow::Owned(format!("{number:.0}")),
        DataRef::Float(number) => Cow::Owned(number.to_string()),
        DataRef::Bool(flag) => Cow::Owned(flag.to_string()),
        DataRef::String(text) => Cow::Borrowed(text),
        DataRef::SharedString(text) => Cow::Borrowed(text),
        DataRef::DateTimeIso(text) | DataRef::DurationIso(text) => Cow::Borrowed(text),
        DataRef::DateTime(stamp) => Cow::Owned(stamp.as_datetime().map_or_else(
            || stamp.as_f64().to_string(),
            |moment| {
                if moment.time() == chrono::NaiveTime::MIN {
                    moment.format("%Y-%m-%d").to_string()
                } else {
                    moment.format("%Y-%m-%d %H:%M:%S").to_string()
                }
            },
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_room_for_a_header_before_refusing_rows_after_the_import_limit() {
        let mut writer = csv::WriterBuilder::new().flexible(true).from_writer(Vec::new());
        let mut row = vec!["value".to_owned()];
        let mut rows_written = MAX_IMPORT_ROWS;
        let mut output_bytes = 0;
        let mut row_payload_bytes = 5;

        write_row(
            &mut writer,
            &mut row,
            &mut rows_written,
            &mut output_bytes,
            &mut row_payload_bytes,
        )
        .unwrap();
        assert_eq!(rows_written, MAX_IMPORT_ROWS + 1);
        assert_eq!(
            write_row(
                &mut writer,
                &mut row,
                &mut rows_written,
                &mut output_bytes,
                &mut row_payload_bytes
            ),
            Err(ImportError::TooManyRows { limit: MAX_IMPORT_ROWS })
        );
        assert_eq!(rows_written, MAX_IMPORT_ROWS + 1);
    }

    #[test]
    fn a_workbook_becomes_comma_separated_text() {
        let mut workbook = rust_xlsxwriter::Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.write_string(0, 0, "id").unwrap();
        sheet.write_string(0, 1, "note").unwrap();
        sheet.write_number(1, 0, 7).unwrap();
        sheet.write_string(1, 1, "a,b").unwrap();
        sheet.write_number(3, 0, 1.5).unwrap();
        let bytes = workbook.save_to_buffer().unwrap();

        assert!(is_xlsx(&bytes));
        let csv = String::from_utf8(xlsx_to_csv(&bytes).unwrap()).unwrap();
        assert_eq!(csv, "id,note\n7,\"a,b\"\n\"\"\n1.5\n");
    }

    #[test]
    fn rejects_cells_with_decreasing_row_numbers() {
        let mut workbook = rust_xlsxwriter::Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.write_number(0, 0, 1).unwrap();
        sheet.write_number(1, 0, 2).unwrap();
        let bytes = workbook.save_to_buffer().unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).unwrap();
            let name = file.name().unwrap().into_owned();
            output
                .start_file(name.clone(), zip::write::SimpleFileOptions::default())
                .unwrap();
            if name == "xl/worksheets/sheet1.xml" {
                let mut xml = String::new();
                std::io::Read::read_to_string(&mut file, &mut xml).unwrap();
                let xml = xml
                    .replace("r=\"A1\"", "r=\"A2\"")
                    .replace("r=\"A2\"><v>2", "r=\"A1\"><v>2");
                std::io::Write::write_all(&mut output, xml.as_bytes()).unwrap();
            } else {
                std::io::copy(&mut file, &mut output).unwrap();
            }
        }
        let bytes = output.finish().unwrap().into_inner();
        assert_eq!(xlsx_to_csv(&bytes), Err(ImportError::NotSpreadsheet));
    }

    #[test]
    fn csv_size_estimate_matches_serialized_fields() {
        for value in ["plain", "a,b", "a\"b", "a\nb", ""] {
            let mut writer = csv::WriterBuilder::new().from_writer(Vec::new());
            writer.write_record([value]).unwrap();
            let output = writer.into_inner().unwrap();
            assert_eq!(row_csv_bytes(field_csv_bytes(value), 1), output.len());
        }
        for fields in [vec![], vec!["", ""], vec!["", "value", ""]] {
            let mut writer = csv::WriterBuilder::new().from_writer(Vec::new());
            writer.write_record(&fields).unwrap();
            let output = writer.into_inner().unwrap();
            let payload = fields.iter().map(|field| field_csv_bytes(field)).sum();
            assert_eq!(row_csv_bytes(payload, fields.len()), output.len());
        }
    }

    #[test]
    fn refuses_a_final_csv_row_that_exceeds_the_import_file_limit() {
        let value = "x".repeat(32_767);
        let mut workbook = rust_xlsxwriter::Workbook::new();
        let sheet = workbook.add_worksheet();
        for row in 0..5 {
            for column in 0..MAX_COLUMNS as u16 {
                sheet.write_string(row, column, &value).unwrap();
            }
        }
        let bytes = workbook.save_to_buffer().unwrap();

        let result = xlsx_to_csv(&bytes);
        match result {
            Err(ImportError::FileTooLarge { limit, .. }) => assert_eq!(limit, MAX_FILE_BYTES),
            Ok(output) => panic!("converted output was {} bytes", output.len()),
            Err(error) => panic!("unexpected import error: {error}"),
        }
    }

    #[test]
    fn refuses_a_declared_shared_string_count_above_the_import_row_limit() {
        let mut workbook = rust_xlsxwriter::Workbook::new();
        workbook.add_worksheet().write_string(0, 0, "value").unwrap();
        let bytes = workbook.save_to_buffer().unwrap();
        let bytes = with_shared_string_count(&bytes, MAX_IMPORT_ROWS + 1);

        assert_eq!(
            xlsx_to_csv(&bytes),
            Err(ImportError::TooManySharedStrings { limit: MAX_IMPORT_ROWS })
        );
    }

    #[test]
    fn refuses_large_shared_string_counts_at_alternate_paths() {
        for name in [
            "other/SHAREDSTRINGS.XML",
            "other\\sharedStrings.xml",
            "sharedStrings.xml",
        ] {
            let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
            output
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            std::io::Write::write_all(&mut output, b"<sst uniqueCount=\"1000001\"/>").unwrap();
            let bytes = output.finish().unwrap().into_inner();
            assert_eq!(
                check_workbook_xml_bounds(&bytes),
                Err(ImportError::TooManySharedStrings { limit: MAX_IMPORT_ROWS })
            );
        }
    }

    fn with_shared_string_count(bytes: &[u8], count: usize) -> Vec<u8> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).unwrap();
            let name = file.name().unwrap().into_owned();
            output
                .start_file(name.clone(), zip::write::SimpleFileOptions::default())
                .unwrap();
            if name == "xl/sharedStrings.xml" {
                let mut xml = String::new();
                std::io::Read::read_to_string(&mut file, &mut xml).unwrap();
                let start = xml.find("uniqueCount=\"").unwrap() + "uniqueCount=\"".len();
                let end = start + xml[start..].find('"').unwrap();
                xml.replace_range(start..end, &count.to_string());
                std::io::Write::write_all(&mut output, xml.as_bytes()).unwrap();
            } else {
                std::io::copy(&mut file, &mut output).unwrap();
            }
        }
        output.finish().unwrap().into_inner()
    }

    #[test]
    fn text_that_is_not_a_workbook_is_refused() {
        assert!(!is_xlsx(b"a,b\n1,2\n"));
        assert_eq!(xlsx_to_csv(b"PK\x03\x04 not really"), Err(ImportError::NotSpreadsheet));
    }

    #[test]
    fn an_empty_workbook_has_no_columns() {
        let mut workbook = rust_xlsxwriter::Workbook::new();
        workbook.add_worksheet();
        let bytes = workbook.save_to_buffer().unwrap();
        assert_eq!(xlsx_to_csv(&bytes), Err(ImportError::NoColumns));
    }
}
