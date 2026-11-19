use super::*;
use crate::export::hex_encode;
use crate::export::test_support::column;

const FORMATS: [ResultFormat; 8] = [
    ResultFormat::Csv,
    ResultFormat::Json,
    ResultFormat::JsonLines,
    ResultFormat::Markdown,
    ResultFormat::Html,
    ResultFormat::Xml,
    ResultFormat::Sql,
    ResultFormat::Xlsx,
];

fn render_row(format: ResultFormat, row: &[Value]) -> Result<Vec<u8>, ExportError> {
    let columns: Vec<_> = (0..row.len()).map(|index| column(&format!("c{index}"))).collect();
    let options = CsvOptions::default();
    let export = ResultExport {
        format,
        csv: &options,
        sql: Some(SqlTarget {
            driver_id: "postgres",
            schema: None,
            table: "items",
        }),
    };
    let mut output = Vec::new();
    let mut writer = writer_for(&export, 1)?;
    writer.begin(&mut output, &columns)?;
    writer.write_row(&mut output, 0, row)?;
    writer.finish(&mut output)?;
    Ok(output)
}

fn text_of(format: ResultFormat, row: &[Value]) -> String {
    String::from_utf8(render_row(format, row).unwrap()).unwrap()
}

fn big_blob() -> Vec<u8> {
    (0..=255u8).cycle().take(70_000).collect()
}

fn text_formats() -> impl Iterator<Item = ResultFormat> {
    FORMATS.into_iter().filter(|format| *format != ResultFormat::Xlsx)
}

#[test]
fn value_honesty_every_text_export_writes_a_binary_payload_past_64_kib_in_full() {
    let blob = big_blob();
    let hex = hex_encode(&blob);
    for format in text_formats() {
        let output = text_of(format, &[Value::Bytes(blob.clone())]);
        let expected = match format {
            ResultFormat::Sql => hex.clone(),
            ResultFormat::Markdown => format!("&#92;x{hex}"),
            _ => format!("\\x{hex}"),
        };
        assert!(output.contains(&expected), "{format:?} lost part of the payload");
    }
}

#[test]
fn value_honesty_a_workbook_refuses_a_binary_payload_beyond_the_cell_limit_instead_of_cutting_it() {
    let refused = render_row(ResultFormat::Xlsx, &[Value::Bytes(big_blob())]);
    assert!(refused.is_err());
    assert!(render_row(ResultFormat::Xlsx, &[Value::Bytes(vec![0xde; 8_000])]).is_ok());
}

#[test]
fn value_honesty_every_export_keeps_a_boolean_distinct_from_text() {
    for format in text_formats() {
        let flag = text_of(format, &[Value::Bool(true), Value::Bool(false)]);
        let text = text_of(format, &[Value::Text("true".into()), Value::Text("false".into())]);
        assert!(flag.contains("true") && flag.contains("false"), "{format:?}: {flag}");
        if matches!(format, ResultFormat::Json | ResultFormat::JsonLines) {
            assert!(flag.contains(": true") || flag.contains(":true"), "{format:?}: {flag}");
            assert!(text.contains("\"true\""), "{format:?}: {text}");
        }
    }
    let flag = render_row(ResultFormat::Xlsx, &[Value::Bool(true)]).unwrap();
    let text = render_row(ResultFormat::Xlsx, &[Value::Text("true".into())]).unwrap();
    assert_ne!(flag, text);
}

#[test]
fn value_honesty_every_export_keeps_a_row_of_only_nulls_as_a_row() {
    let nulls = vec![Value::Null; 3];
    for format in FORMATS {
        let with_row = render_row(format, &nulls).unwrap();
        let columns: Vec<_> = (0..3).map(|index| column(&format!("c{index}"))).collect();
        let options = CsvOptions::default();
        let export = ResultExport {
            format,
            csv: &options,
            sql: Some(SqlTarget {
                driver_id: "postgres",
                schema: None,
                table: "items",
            }),
        };
        let mut without_row = Vec::new();
        let mut writer = writer_for(&export, 0).unwrap();
        writer.begin(&mut without_row, &columns).unwrap();
        writer.finish(&mut without_row).unwrap();
        assert_ne!(with_row, without_row, "{format:?} dropped the all-NULL row");
    }
    assert_eq!(text_of(ResultFormat::Csv, &nulls), "c0,c1,c2\n,,\n");
    assert_eq!(text_of(ResultFormat::Json, &nulls).matches("null").count(), 3);
    assert!(text_of(ResultFormat::Sql, &nulls).contains("VALUES (NULL, NULL, NULL)"));
}

#[test]
fn value_honesty_every_export_writes_a_zoned_timestamp_with_its_offset() {
    let stamp = Value::TimestampTz("2026-09-27T12:34:56.123456789+05:30".parse().unwrap());
    for format in text_formats() {
        let output = text_of(format, std::slice::from_ref(&stamp));
        assert!(
            output.contains("2026-09-27T07:04:56.123456789+00:00"),
            "{format:?}: {output}"
        );
    }
    let workbook = render_row(ResultFormat::Xlsx, std::slice::from_ref(&stamp)).unwrap();
    let naive = Value::DateTime("2026-09-27T07:04:56.123456789".parse().unwrap());
    assert_ne!(workbook, render_row(ResultFormat::Xlsx, &[naive]).unwrap());
}
