use std::path::Path;

const MAX_CHANGED_LINES: usize = 20;

pub fn assert_golden(path: &Path, actual: &[String]) {
    let recording = std::env::var("BOOKIE_RECORD").as_deref() == Ok("1");
    let force = std::env::var("BOOKIE_RECORD_FORCE").as_deref() == Ok("1");
    let ci = std::env::var_os("CI").is_some();
    if let Err(error) = compare(path, actual, recording, force, ci) {
        panic!("{error}");
    }
}

fn compare(path: &Path, actual: &[String], recording: bool, force: bool, ci: bool) -> Result<(), String> {
    if recording && ci {
        return Err("golden recording is disabled in CI".into());
    }
    let expected = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && recording => String::new(),
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    let before: Vec<_> = expected.lines().collect();
    let count = before.len().max(actual.len());
    let mut diff = Vec::new();
    for index in 0..count {
        let old = before.get(index).copied();
        let new = actual.get(index).map(String::as_str);
        if old != new {
            diff.push(format!("line {}: expected {:?}; actual {:?}", index + 1, old, new));
        }
    }
    if diff.is_empty() && path.exists() {
        return Ok(());
    }
    if !recording {
        return Err(format!("golden mismatch in {}:\n{}", path.display(), diff.join("\n")));
    }
    if diff.len() > MAX_CHANGED_LINES && !force {
        return Err(format!(
            "{} lines changed; set BOOKIE_RECORD_FORCE=1 to record more than {MAX_CHANGED_LINES}",
            diff.len()
        ));
    }
    let mut output = actual.join("\n");
    if !actual.is_empty() {
        output.push('\n');
    }
    std::fs::write(path, output).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_and_mismatching_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("golden");
        std::fs::write(&path, "a\nb\n").unwrap();
        assert!(compare(&path, &["a".into(), "b".into()], false, false, false).is_ok());
        assert!(
            compare(&path, &["a".into(), "c".into()], false, false, false)
                .unwrap_err()
                .contains("line 2")
        );
    }

    #[test]
    fn recording_requires_permission_and_is_forbidden_in_ci() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("golden");
        let lines = vec!["a".to_string()];
        assert!(compare(&path, &lines, false, false, false).is_err());
        assert!(compare(&path, &lines, true, false, true).is_err());
        assert!(!path.exists());
        compare(&path, &lines, true, false, false).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "a\n");
    }

    #[test]
    fn large_recording_needs_force() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("golden");
        let lines = vec!["x".to_string(); MAX_CHANGED_LINES + 1];
        assert!(compare(&path, &lines, true, false, false).is_err());
        assert!(!path.exists());
        compare(&path, &lines, true, true, false).unwrap();
    }
}
