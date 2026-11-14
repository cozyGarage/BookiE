use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroizing;

use crate::DumpError;

const PREFIX: &str = "bookie-pgpass-";
const LEGACY_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

fn remove_abandoned(runtime_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(runtime_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(rest) = name.to_str().and_then(|name| name.strip_prefix(PREFIX)) else {
            continue;
        };
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let owner_gone = match rest.strip_prefix('p').and_then(|rest| rest.split('-').next()) {
            Some(pid) if pid.parse::<u32>().is_ok() => !Path::new("/proc").join(pid).exists(),
            _ => entry
                .metadata()
                .and_then(|meta| meta.modified())
                .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age > LEGACY_MAX_AGE)),
        };
        if owner_gone {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[derive(Debug)]
pub(crate) struct PassFile {
    path: PathBuf,
}

impl PassFile {
    pub(crate) fn create(runtime_dir: &Path, password: &SecretString) -> Result<Self, DumpError> {
        let line = Zeroizing::new(passfile_line(password.expose_secret())?);
        remove_abandoned(runtime_dir);
        let path = runtime_dir.join(format!("{PREFIX}p{}-{}", std::process::id(), uuid::Uuid::new_v4()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        let passfile = Self { path };
        file.write_all(line.as_bytes())?;
        file.sync_all()?;
        Ok(passfile)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PassFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn passfile_line(password: &str) -> Result<String, DumpError> {
    if password.contains(['\n', '\r']) {
        return Err(DumpError::Unsupported("the password contains a line break"));
    }
    let mut line = String::with_capacity(password.len() + 10);
    line.push_str("*:*:*:*:");
    for ch in password.chars() {
        if matches!(ch, ':' | '\\') {
            line.push('\\');
        }
        line.push(ch);
    }
    line.push('\n');
    Ok(line)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn the_password_is_escaped_for_libpq() {
        assert_eq!(passfile_line(r"a:b\c").unwrap(), "*:*:*:*:a\\:b\\\\c\n");
    }

    #[test]
    fn a_password_with_a_line_break_is_refused() {
        assert!(matches!(passfile_line("a\nb"), Err(DumpError::Unsupported(_))));
    }

    fn write(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, "*:*:*:*:old\n").unwrap();
        path
    }

    #[test]
    fn a_new_password_file_sweeps_the_ones_a_crash_left_behind() {
        let dir = tempfile::tempdir().unwrap();
        let dead = write(dir.path(), "bookie-pgpass-p4294967000-aaaa");
        let live = write(dir.path(), &format!("bookie-pgpass-p{}-bbbb", std::process::id()));
        let old_legacy = write(dir.path(), "bookie-pgpass-cccc");
        let two_days_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 24 * 3600);
        std::fs::File::options()
            .write(true)
            .open(&old_legacy)
            .unwrap()
            .set_modified(two_days_ago)
            .unwrap();
        let new_legacy = write(dir.path(), "bookie-pgpass-dddd");
        let unrelated = write(dir.path(), "something-else");

        let _fresh = PassFile::create(dir.path(), &SecretString::from("secret")).unwrap();

        assert!(!dead.exists(), "a file whose process is gone is removed");
        assert!(!old_legacy.exists(), "an old file without a process id is removed");
        assert!(live.exists(), "a file of a running process is kept");
        assert!(new_legacy.exists(), "a recent file without a process id is kept");
        assert!(unrelated.exists(), "other files are never touched");
    }

    #[test]
    fn the_file_is_owner_only_and_removed_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let passfile = PassFile::create(dir.path(), &SecretString::from("secret")).unwrap();
        let path = passfile.path().to_path_buf();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        drop(passfile);
        assert!(!path.exists());
    }
}
