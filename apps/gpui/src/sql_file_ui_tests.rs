use super::{sql_file_destination, sql_file_source};

fn scratch(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("bookie-sqlfile-{}-{name}", std::process::id()))
}

#[test]
fn a_saved_script_gets_sql_when_it_has_no_extension_and_never_overwrites() {
    let plain = scratch("fresh");
    assert_eq!(
        sql_file_destination(&plain.display().to_string()).unwrap(),
        scratch("fresh.sql")
    );
    let keep = scratch("keep.sql");
    std::fs::write(&keep, "SELECT 1;").unwrap();
    assert!(
        sql_file_destination(&keep.display().to_string())
            .unwrap_err()
            .contains("already exists")
    );
    std::fs::remove_file(&keep).ok();
}

#[test]
fn a_script_path_must_be_absolute_and_name_a_file() {
    assert!(sql_file_destination("").is_err());
    assert!(sql_file_destination("query.sql").unwrap_err().contains("absolute"));
    assert!(
        sql_file_destination(&format!("{}/", std::env::temp_dir().display()))
            .unwrap_err()
            .contains("file name")
    );
    assert!(sql_file_source("query.sql").unwrap_err().contains("absolute"));
    assert!(sql_file_source("/not/here.sql").is_err());
    assert!(sql_file_source(&std::env::temp_dir().display().to_string()).is_err());
}

#[test]
fn an_existing_file_is_a_valid_source() {
    let file = scratch("source.sql");
    std::fs::write(&file, "SELECT 2;").unwrap();
    assert_eq!(sql_file_source(&file.display().to_string()).unwrap(), file);
    std::fs::remove_file(&file).ok();
}
