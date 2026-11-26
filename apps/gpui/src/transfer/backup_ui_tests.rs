use super::{backup_destination, restore_confirmed, restore_source};

#[test]
fn a_backup_name_without_an_extension_gets_dump_and_a_doubled_one_is_collapsed() {
    let directory = std::env::temp_dir().join(format!("bookie-backup-ui-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let plain = directory.join("night");
    assert_eq!(
        backup_destination(&plain.display().to_string()).unwrap(),
        directory.join("night.dump")
    );
    let doubled = directory.join("night.dump.dump");
    assert_eq!(
        backup_destination(&doubled.display().to_string()).unwrap(),
        directory.join("night.dump")
    );
    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn a_backup_never_overwrites_and_needs_a_name() {
    assert!(backup_destination("   ").is_err());
    let existing = std::env::temp_dir().join(format!("bookie-backup-exists-{}.dump", std::process::id()));
    std::fs::write(&existing, b"x").unwrap();
    let error = backup_destination(&existing.display().to_string()).unwrap_err();
    assert!(error.contains("already exists"), "{error}");
    std::fs::remove_file(&existing).ok();
}

#[test]
fn a_restore_needs_an_existing_file() {
    assert!(restore_source("").is_err());
    assert!(restore_source("/definitely/not/here.dump").is_err());
    let file = std::env::temp_dir().join(format!("bookie-restore-src-{}.dump", std::process::id()));
    std::fs::write(&file, b"PGDMP").unwrap();
    assert_eq!(restore_source(&file.display().to_string()).unwrap(), file);
    assert!(
        restore_source(&std::env::temp_dir().display().to_string()).is_err(),
        "a directory is not a backup"
    );
    std::fs::remove_file(&file).ok();
}

#[test]
fn only_the_exact_word_confirms_a_restore() {
    assert!(restore_confirmed("RESTORE"));
    assert!(restore_confirmed("  RESTORE  "));
    assert!(!restore_confirmed("restore"));
    assert!(!restore_confirmed(""));
    assert!(!restore_confirmed("RESTORE NOW"));
}

#[test]
fn relative_paths_and_other_users_homes_are_refused_before_anything_is_dispatched() {
    for typed in ["out", "backups/x.dump", "./x.dump", "~other/x.dump"] {
        let error = backup_destination(typed).unwrap_err();
        assert!(error.contains("absolute"), "{typed}: {error}");
        let error = restore_source(typed).unwrap_err();
        assert!(error.contains("absolute"), "{typed}: {error}");
    }
}

#[test]
fn a_folder_style_name_is_not_a_backup_file() {
    let directory = std::env::temp_dir().join(format!("bookie-backup-folder-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let typed = format!("{}/", directory.display());
    assert!(backup_destination(&typed).unwrap_err().contains("file name"));
    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn a_restore_job_is_built_only_with_the_word_and_an_existing_absolute_file() {
    let file = std::env::temp_dir().join(format!("bookie-job-for-{}.dump", std::process::id()));
    std::fs::write(&file, b"PGDMP").unwrap();
    let typed = file.display().to_string();
    assert!(super::job_for(true, &typed, "").unwrap_err().contains("RESTORE"));
    assert!(super::job_for(true, &typed, "restore").is_err());
    assert!(matches!(
        super::job_for(true, &typed, "RESTORE"),
        Ok(bookie_app_services::database_backup::BackupJob::Restore { .. })
    ));
    assert!(super::job_for(true, "relative.dump", "RESTORE").is_err());
    assert!(matches!(
        super::job_for(false, &format!("{typed}.new"), ""),
        Ok(bookie_app_services::database_backup::BackupJob::Backup { .. })
    ));
    std::fs::remove_file(&file).ok();
}
