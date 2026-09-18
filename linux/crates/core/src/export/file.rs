use std::io;
use std::path::Path;

use super::{CsvOptions, json_field_names, render_csv, row_to_json_object, write_atomically_checked};
use crate::QueryResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultFormat {
    Csv,
    Json,
}

/// Export an already materialized result. Extra memory is bounded to one row.
/// Cancellation is cooperative; publishing a completed file is the commit point.
pub fn write_result_file(
    path: &Path,
    result: &QueryResult,
    format: ResultFormat,
    options: &CsvOptions,
    cancelled: impl Fn() -> bool,
    progress: impl Fn(usize),
) -> io::Result<()> {
    let check = || {
        if cancelled() {
            Err(io::Error::new(io::ErrorKind::Interrupted, "Export cancelled"))
        } else {
            Ok(())
        }
    };
    check()?;
    write_atomically_checked(
        path,
        |mut output| {
            let names = json_field_names(&result.columns);
            let row_options = CsvOptions {
                header_row: false,
                ..options.clone()
            };
            match format {
                ResultFormat::Csv => output.write_all(render_csv(&result.columns, &[], options).as_bytes())?,
                ResultFormat::Json => output.write_all(b"[\n")?,
            }
            for (index, row) in result.rows.iter().enumerate() {
                check()?;
                match format {
                    ResultFormat::Csv => {
                        output.write_all(render_csv(&[], std::slice::from_ref(row), &row_options).as_bytes())?
                    }
                    ResultFormat::Json => {
                        if index != 0 {
                            output.write_all(b",\n")?;
                        }
                        serde_json::to_writer(&mut output, &row_to_json_object(&names, row))?;
                    }
                }
                progress(index + 1);
            }
            if format == ResultFormat::Json {
                output.write_all(b"\n]\n")?;
            }
            Ok(())
        },
        check,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ColumnInfo, Value};

    fn result() -> QueryResult {
        QueryResult {
            columns: ["id", "id", "id_2"]
                .map(|name| ColumnInfo {
                    name: name.into(),
                    data_type: "text".into(),
                    nullable: true,
                    primary_key: false,
                    is_auto_increment: false,
                    default_value: None,
                    is_generated: false,
                    comment: None,
                })
                .to_vec(),
            rows: vec![vec![
                Value::Bytes(vec![0, 255]),
                Value::Text("=1+1".into()),
                Value::Int(-42),
            ]],
            truncated: false,
        }
    }

    #[test]
    fn file_and_clipboard_agree_and_cancellation_preserves_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("result");
        let data = result();
        write_result_file(
            &path,
            &data,
            ResultFormat::Json,
            &CsvOptions::default(),
            || false,
            |_| {},
        )
        .unwrap();
        let actual: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let expected: serde_json::Value =
            serde_json::from_str(&super::super::render_json(&data.columns, &data.rows)).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual[0]["id"], "\\x00ff");
        let before = std::fs::read(&path).unwrap();
        let cancel = std::cell::Cell::new(false);
        let error = write_result_file(
            &path,
            &data,
            ResultFormat::Csv,
            &CsvOptions::default(),
            || cancel.get(),
            |_| cancel.set(true),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn safe_csv_preserves_numbers_and_raw_mode_preserves_text() {
        let data = result();
        let safe = render_csv(&data.columns, &data.rows, &CsvOptions::default());
        assert!(safe.contains("\"'=1+1\""));
        assert!(safe.contains(",-42\n"));
        let raw = render_csv(
            &data.columns,
            &data.rows,
            &CsvOptions {
                sanitize_formulas: false,
                ..Default::default()
            },
        );
        assert!(raw.contains(",=1+1,-42\n"));
    }
}
