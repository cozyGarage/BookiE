use super::*;
use std::io::Write;

fn options() -> CsvImportOptions {
    CsvImportOptions::default()
}

#[test]
fn csv_format_conversion_preserves_delimiter_and_header_choice() {
    let format = CsvFormat {
        delimiter: CsvDelimiter::Pipe,
        has_header: false,
    };

    let options = CsvImportOptions::from(format);

    assert_eq!(options.delimiter, CsvDelimiter::Pipe);
    assert!(!options.has_header);
    assert!(options.null_marker.is_empty());
}

#[test]
fn public_import_limits_keep_their_documented_units() {
    assert_eq!(MAX_FILE_BYTES, 67_108_864);
    assert_eq!(MAX_COLUMNS, 512);
    assert_eq!(MAX_FIELD_BYTES, 2_097_154);
    assert_eq!(MAX_IMPORT_ROWS, 1_000_000);
    assert_eq!(MAX_PREVIEW_ROWS, 50);
}

#[test]
fn file_size_limit_accepts_exact_boundary_and_refuses_one_byte_more() {
    assert_eq!(check_file_size(MAX_FILE_BYTES), Ok(()));
    assert_eq!(
        check_file_size(MAX_FILE_BYTES + 1),
        Err(ImportError::FileTooLarge {
            size: MAX_FILE_BYTES + 1,
            limit: MAX_FILE_BYTES,
        })
    );
}

#[test]
fn bounded_file_read_rejects_growth_past_the_limit() {
    let mut reader = std::io::Cursor::new(b"four extra bytes");
    let error = read_bounded(&mut reader, 3).expect_err("refuse extra byte");

    assert_eq!(error, ImportError::FileTooLarge { size: 4, limit: 3 });
    assert_eq!(reader.position(), 4);
}

#[test]
fn read_csv_file_observes_delimiter_header_and_preview_options() {
    let mut file = tempfile::NamedTempFile::new().expect("temporary csv file");
    file.write_all(b"id\tname\n1\tAda\n2\tGrace\n")
        .expect("write csv fixture");
    let options = CsvImportOptions {
        delimiter: CsvDelimiter::Tab,
        has_header: true,
        null_marker: "\\N".into(),
        ..CsvImportOptions::default()
    };

    let sheet = read_csv_file(file.path(), &options, Some(1)).expect("read from path");

    assert_eq!(sheet.headers, vec!["id", "name"]);
    assert_eq!(sheet.rows, vec![vec!["1", "Ada"]]);
    assert!(sheet.truncated);
}

#[test]
fn read_csv_file_refuses_oversized_sparse_files_from_metadata() {
    let file = tempfile::NamedTempFile::new().expect("temporary csv file");
    file.as_file().set_len(MAX_FILE_BYTES + 1).expect("make sparse file");

    assert_eq!(
        read_csv_file(file.path(), &options(), None),
        Err(ImportError::FileTooLarge {
            size: MAX_FILE_BYTES + 1,
            limit: MAX_FILE_BYTES,
        })
    );
}

fn column(name: &str, data_type: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        nullable: true,
        primary_key: false,
        is_auto_increment: false,
        default_value: None,
        is_generated: false,
        comment: None,
        collation: None,
        enum_type: None,
        domain_type: None,
    }
}

#[test]
fn a_header_row_names_the_fields() {
    let sheet = read_csv(b"id,name\n1,ada\n", &options(), None).expect("read");

    assert_eq!(sheet.headers, vec!["id", "name"]);
    assert_eq!(sheet.rows, vec![vec!["1".to_owned(), "ada".to_owned()]]);
    assert!(!sheet.truncated);
}

#[test]
fn a_file_with_no_header_gets_names_from_the_positions() {
    let mut options = options();
    options.has_header = false;

    let sheet = read_csv(b"1,ada\n2,grace\n", &options, None).expect("read");

    assert_eq!(sheet.headers, vec!["Column 1", "Column 2"]);
    assert_eq!(sheet.rows.len(), 2);
}

#[test]
fn a_quoted_field_keeps_its_delimiters_and_newlines() {
    let sheet = read_csv(b"a,b\n\"one, two\",\"line\nbreak\"\n", &options(), None).expect("read");

    assert_eq!(sheet.rows[0][0], "one, two");
    assert_eq!(sheet.rows[0][1], "line\nbreak");
}

#[test]
fn a_doubled_quote_inside_a_quoted_field_is_one_quote() {
    let sheet = read_csv(b"a\n\"say \"\"hi\"\"\"\n", &options(), None).expect("read");

    assert_eq!(sheet.rows[0][0], "say \"hi\"");
}

#[test]
fn carriage_returns_do_not_survive_into_the_last_field() {
    let sheet = read_csv(b"a,b\r\n1,two\r\n", &options(), None).expect("read");

    assert_eq!(sheet.headers, vec!["a", "b"]);
    assert_eq!(sheet.rows[0], vec!["1".to_owned(), "two".to_owned()]);
}

/// The csv crate strips a leading UTF-8 byte order mark itself. If it
/// ever stopped, the first header name would start with U+FEFF and
/// match no column, so the import must keep testing for it.
#[test]
fn a_byte_order_mark_is_not_part_of_the_first_column_name() {
    let sheet = read_csv("\u{feff}id,name\n1,ada\n".as_bytes(), &options(), None).expect("read");

    assert_eq!(sheet.headers, vec!["id", "name"]);
}

#[test]
fn a_short_row_is_padded_rather_than_refused() {
    let sheet = read_csv(b"a,b,c\n1\n", &options(), None).expect("read");

    assert_eq!(sheet.rows[0], vec!["1".to_owned(), String::new(), String::new()]);
}

#[test]
fn a_long_row_keeps_the_fields_the_header_did_not_name() {
    let sheet = read_csv(b"a,b\n1,2,3\n", &options(), None).expect("read");

    assert_eq!(sheet.rows[0], vec!["1".to_owned(), "2".to_owned(), "3".to_owned()]);
}

#[test]
fn a_preview_stops_at_the_limit_and_says_so() {
    let mut file = String::from("a\n");
    for index in 0..10 {
        file.push_str(&format!("{index}\n"));
    }

    let sheet = read_csv(file.as_bytes(), &options(), Some(3)).expect("read");

    assert_eq!(sheet.rows.len(), 3);
    assert!(sheet.truncated);
}

#[test]
fn a_file_that_fits_the_limit_is_not_called_truncated() {
    let sheet = read_csv(b"a\n1\n2\n", &options(), Some(50)).expect("read");

    assert_eq!(sheet.rows.len(), 2);
    assert!(!sheet.truncated);
}

#[test]
fn a_file_that_is_not_utf8_says_where_it_stops_being_text() {
    let error = read_csv(b"a,b\n1,\xff\n", &options(), None).expect_err("not utf-8");

    assert_eq!(error, ImportError::NotUtf8 { offset: 6 });
}

#[test]
fn an_empty_file_has_no_columns() {
    assert_eq!(read_csv(b"", &options(), None), Err(ImportError::NoColumns));
}

#[test]
fn a_field_over_the_length_limit_is_refused_and_names_the_line() {
    let mut file = String::from("a\n");
    file.push_str(&"x".repeat(MAX_FIELD_BYTES + 1));
    file.push('\n');

    let error = read_csv(file.as_bytes(), &options(), None).expect_err("too long");

    assert_eq!(
        error,
        ImportError::FieldTooLong {
            line: 2,
            limit: MAX_FIELD_BYTES
        }
    );
}

#[test]
fn a_field_at_the_length_limit_is_accepted() {
    let mut file = String::from("a\n");
    file.push_str(&"x".repeat(MAX_FIELD_BYTES));
    file.push('\n');

    let sheet = read_csv(file.as_bytes(), &options(), None).expect("exact maximum field");

    assert_eq!(sheet.rows[0][0].len(), MAX_FIELD_BYTES);
}

#[test]
fn a_file_over_the_size_limit_is_refused_before_it_is_parsed() {
    let bytes = vec![b'a'; (MAX_FILE_BYTES + 1) as usize];

    let error = read_csv(&bytes, &options(), None).expect_err("too large");

    assert!(matches!(error, ImportError::FileTooLarge { limit, .. } if limit == MAX_FILE_BYTES));
}

#[test]
fn more_columns_than_the_limit_are_refused() {
    let header = (0..=MAX_COLUMNS).map(|index| format!("c{index}")).collect::<Vec<_>>();

    let error = read_csv(header.join(",").as_bytes(), &options(), None).expect_err("too wide");

    assert!(matches!(error, ImportError::TooManyColumns { limit, .. } if limit == MAX_COLUMNS));
}

#[test]
fn the_maximum_column_count_is_accepted() {
    let header = (0..MAX_COLUMNS).map(|index| format!("c{index}")).collect::<Vec<_>>();

    let sheet = read_csv(header.join(",").as_bytes(), &options(), None).expect("exact maximum columns");

    assert_eq!(sheet.headers.len(), MAX_COLUMNS);
}

#[test]
fn a_semicolon_file_is_detected_as_semicolon_delimited() {
    let format = detect_format(b"id;name;age\n1;ada;36\n2;grace;30\n");

    assert_eq!(format.delimiter, CsvDelimiter::Semicolon);
    assert!(format.has_header);
}

#[test]
fn a_comma_file_is_detected_as_comma_delimited() {
    let format = detect_format(b"id,name,age\n1,ada,36\n");

    assert_eq!(format.delimiter, CsvDelimiter::Comma);
    assert!(format.has_header);
}

#[test]
fn a_tab_file_is_detected_as_tab_delimited() {
    let format = detect_format(b"id\tname\n1\tada\n");

    assert_eq!(format.delimiter, CsvDelimiter::Tab);
    assert!(format.has_header);
}

#[test]
fn a_file_whose_first_row_is_data_is_not_read_as_having_a_header() {
    let format = detect_format(b"1,ada\n2,grace\n3,alan\n");

    assert!(!format.has_header);
}

#[test]
fn a_single_column_file_falls_back_to_the_comma() {
    let format = detect_format(b"name\nada\ngrace\n");

    assert_eq!(format.delimiter, CsvDelimiter::Comma);
}

#[test]
fn a_mapping_matches_on_name_whatever_the_case_or_spacing() {
    let headers = vec!["ID".to_owned(), " Name ".to_owned(), "extra".to_owned()];
    let columns = vec![
        column("name", "text"),
        column("id", "bigint"),
        column("missing", "text"),
    ];

    assert_eq!(suggest_mapping(&headers, &columns), vec![Some(1), Some(0), None]);
}

#[test]
fn exact_header_names_take_precedence_over_case_insensitive_matches() {
    let headers = vec!["name".to_owned(), "Name".to_owned()];
    let columns = vec![column("Name", "text"), column("name", "text")];

    assert_eq!(suggest_mapping(&headers, &columns), vec![Some(1), Some(0)]);
}

#[test]
fn a_case_insensitive_match_does_not_consume_an_exact_match_header() {
    let headers = vec!["Name".to_owned()];
    let columns = vec![column("name", "text"), column("Name", "text")];

    assert_eq!(suggest_mapping(&headers, &columns), vec![None, Some(0)]);
}

#[test]
fn quote_character_can_be_single_or_absent() {
    let single = CsvImportOptions {
        quote: CsvImportQuote::Single,
        ..options()
    };
    let sheet = read_csv(b"a,b\n'x,y',\"z\"\n", &single, None).expect("read");
    assert_eq!(sheet.rows, vec![vec!["x,y".to_string(), "\"z\"".to_string()]]);
    let none = CsvImportOptions {
        quote: CsvImportQuote::None,
        ..options()
    };
    let sheet = read_csv(b"a,b\n\"x\",y\n", &none, None).expect("read");
    assert_eq!(sheet.rows, vec![vec!["\"x\"".to_string(), "y".to_string()]]);
}

#[test]
fn backslash_escape_keeps_an_escaped_quote_inside_the_field() {
    let text = b"a\n\"say \\\"hi\\\"\"\n";
    let escaped = CsvImportOptions {
        backslash_escape: true,
        ..options()
    };
    assert_eq!(
        read_csv(text, &escaped, None).expect("read").rows,
        vec![vec!["say \"hi\"".to_string()]]
    );
}

#[test]
fn other_encodings_decode_before_parsing() {
    let windows = CsvImportOptions {
        encoding: CsvImportEncoding::Windows1252,
        ..options()
    };
    let sheet = read_csv(b"name\ncaf\xe9\n", &windows, None).expect("read");
    assert_eq!(sheet.rows, vec![vec!["caf\u{e9}".to_string()]]);
    let utf16: Vec<u8> = "a\n\u{e9}\n".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let utf16_options = CsvImportOptions {
        encoding: CsvImportEncoding::Utf16Le,
        ..options()
    };
    assert_eq!(
        read_csv(&utf16, &utf16_options, None).expect("read").rows,
        vec![vec!["\u{e9}".to_string()]]
    );
    let broken = CsvImportOptions {
        encoding: CsvImportEncoding::Utf16Le,
        ..options()
    };
    assert!(matches!(
        read_csv(&[0x00, 0xd8, 0x41], &broken, None),
        Err(ImportError::NotDecodable { .. })
    ));
}
