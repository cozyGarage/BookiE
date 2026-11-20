use std::path::PathBuf;

use super::file::ResultFormat;
use super::format::{insert_target, normalize_duplicate_extension, offered_formats, suggested_file_name};

#[test]
fn every_format_has_a_label_a_unique_extension_and_a_media_type() {
    let mut seen = Vec::new();
    for format in ResultFormat::ALL {
        assert!(!format.label().is_empty());
        assert!(!format.mime_type().is_empty());
        assert!(!seen.contains(&format.extension()), "{}", format.extension());
        seen.push(format.extension());
        assert_eq!(ResultFormat::from_extension(format.extension()), Some(format));
    }
    assert_eq!(ResultFormat::ALL.len(), 8);
}

#[test]
fn the_extension_lookup_is_case_insensitive_and_knows_the_common_aliases() {
    assert_eq!(ResultFormat::from_extension("CSV"), Some(ResultFormat::Csv));
    assert_eq!(ResultFormat::from_extension("ndjson"), Some(ResultFormat::JsonLines));
    assert_eq!(ResultFormat::from_extension("markdown"), Some(ResultFormat::Markdown));
    assert_eq!(ResultFormat::from_extension("htm"), Some(ResultFormat::Html));
    assert_eq!(ResultFormat::from_extension("txt"), None);
    assert_eq!(ResultFormat::from_extension(""), None);
}

#[test]
fn only_csv_shows_the_csv_options() {
    for format in ResultFormat::ALL {
        assert_eq!(format.shows_csv_options(), format == ResultFormat::Csv, "{format:?}");
    }
}

#[test]
fn sql_is_offered_only_where_literals_exist_and_a_workbook_only_when_the_size_is_known() {
    let with_sql = offered_formats("postgres", true);
    assert!(with_sql.contains(&ResultFormat::Sql));
    assert_eq!(with_sql.len(), 8);
    let without_sql = offered_formats("mongodb", true);
    assert!(!without_sql.contains(&ResultFormat::Sql));
    assert_eq!(without_sql[0], ResultFormat::Csv);
    assert!(!offered_formats("sqlite", false).contains(&ResultFormat::Xlsx));
    assert!(offered_formats("sqlite", true).contains(&ResultFormat::Xlsx));
}

#[test]
fn a_suggested_name_swaps_a_known_extension_and_keeps_anything_else() {
    assert_eq!(suggested_file_name("customers", ResultFormat::Csv), "customers.csv");
    assert_eq!(
        suggested_file_name("customers.csv", ResultFormat::Json),
        "customers.json"
    );
    assert_eq!(suggested_file_name("customers.md", ResultFormat::Sql), "customers.sql");
    assert_eq!(suggested_file_name("a.b", ResultFormat::Xml), "a.b.xml");
    assert_eq!(suggested_file_name(".csv", ResultFormat::Json), ".csv.json");
}

#[test]
fn a_file_chooser_duplicate_extension_is_removed_exactly_once() {
    let fix = |path: &str, ext: &str| normalize_duplicate_extension(PathBuf::from(path), ext);
    assert_eq!(fix("/tmp/report.sql.sql", "sql"), PathBuf::from("/tmp/report.sql"));
    assert_eq!(
        fix("/tmp/report.sql.sql.sql", "sql"),
        PathBuf::from("/tmp/report.sql.sql")
    );
    assert_eq!(fix("/tmp/report.txt.sql", "sql"), PathBuf::from("/tmp/report.txt.sql"));
    assert_eq!(fix("/tmp/", "sql"), PathBuf::from("/tmp/"));
}

#[test]
fn an_insert_target_splits_a_schema_only_when_both_halves_exist() {
    assert_eq!(insert_target("public.orders"), (Some("public".into()), "orders".into()));
    assert_eq!(insert_target("orders"), (None, "orders".into()));
    assert_eq!(insert_target(".orders"), (None, ".orders".into()));
    assert_eq!(insert_target("public."), (None, "public.".into()));
}
