use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::DumpError;

const SEARCH_DIRS: [&str; 2] = ["/usr/bin", "/usr/local/bin"];
const VERSION_TIMEOUT: Duration = Duration::from_secs(30);
const SNIPPET_CHARS: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientTool {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) major: u32,
}

impl ClientTool {
    pub async fn locate(name: &str, configured: Option<&Path>) -> Result<Self, DumpError> {
        let missing = || DumpError::ToolMissing { tool: name.to_owned() };
        let path = match configured {
            Some(path) => executable(path).then(|| path.to_path_buf()).ok_or_else(missing)?,
            None => SEARCH_DIRS
                .iter()
                .map(|dir| Path::new(dir).join(name))
                .find(|path| executable(path))
                .ok_or_else(missing)?,
        };
        let major = read_major(name, &path).await?;
        Ok(Self {
            name: name.to_owned(),
            path,
            major,
        })
    }

    pub fn major(&self) -> u32 {
        self.major
    }

    pub(crate) fn require_at_least(&self, server_major: u32) -> Result<(), DumpError> {
        if self.major >= server_major {
            return Ok(());
        }
        Err(DumpError::ToolTooOld {
            tool: self.name.clone(),
            tool_major: self.major,
            server_major,
        })
    }

    pub(crate) fn require_at_most(&self, server_major: u32) -> Result<(), DumpError> {
        if self.major <= server_major {
            return Ok(());
        }
        Err(DumpError::ToolTooNew {
            tool: self.name.clone(),
            tool_major: self.major,
            server_major,
        })
    }
}

fn executable(path: &Path) -> bool {
    path.is_absolute()
        && std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

async fn read_major(name: &str, path: &Path) -> Result<u32, DumpError> {
    read_major_within(name, path, VERSION_TIMEOUT).await
}

async fn read_major_within(name: &str, path: &Path, limit: Duration) -> Result<u32, DumpError> {
    let unknown = |reason: String| DumpError::ToolVersionUnknown {
        tool: name.to_owned(),
        reason,
    };
    let output = Command::new(path)
        .arg("--version")
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(limit, output)
        .await
        .map_err(|_| unknown(format!("it did not answer within {} s", limit.as_secs())))??;
    let printed = String::from_utf8_lossy(&output.stdout);
    parse_major(&printed).ok_or_else(|| {
        let errors = String::from_utf8_lossy(&output.stderr);
        unknown(format!(
            "{}, printed {:?}, error output {:?}",
            output.status,
            snippet(&printed),
            snippet(&errors)
        ))
    })
}

fn snippet(text: &str) -> String {
    text.trim().chars().take(SNIPPET_CHARS).collect()
}

fn parse_major(version: &str) -> Option<u32> {
    let rest = version.split_once("(PostgreSQL)")?.1.trim_start();
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_major_version_is_read_from_the_version_line() {
        assert_eq!(parse_major("pg_dump (PostgreSQL) 18.6\n"), Some(18));
        assert_eq!(parse_major("pg_dump (PostgreSQL) 17beta1"), Some(17));
        assert_eq!(parse_major("pg_dump (PostgreSQL) 12.20 (Debian 12.20-1)"), Some(12));
        assert_eq!(parse_major("mysqldump  Ver 10.19"), None);
    }

    #[test]
    fn a_relative_or_missing_path_is_not_executable() {
        assert!(!executable(Path::new("pg_dump")));
        assert!(!executable(Path::new("/nonexistent/pg_dump")));
    }

    #[test]
    fn client_and_server_versions_must_be_compatible() {
        let tool = ClientTool {
            name: "pg_dump".into(),
            path: PathBuf::from("/usr/bin/pg_dump"),
            major: 16,
        };
        assert!(tool.require_at_least(16).is_ok());
        assert!(matches!(tool.require_at_least(17), Err(DumpError::ToolTooOld { .. })));
        assert!(tool.require_at_most(16).is_ok());
        assert!(matches!(tool.require_at_most(15), Err(DumpError::ToolTooNew { .. })));
    }

    async fn probe(script: &str, limit: Duration) -> Result<u32, DumpError> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tool");
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        for _ in 0..40 {
            match read_major_within("pg_dump", &path, limit).await {
                Err(DumpError::Io(error)) if error.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                other => return other,
            }
        }
        panic!("the probe executable stayed busy");
    }

    #[tokio::test]
    async fn a_probe_that_answers_in_time_gives_its_major_version() {
        let major = probe("#!/bin/sh\necho 'pg_dump (PostgreSQL) 17.2'\n", Duration::from_secs(5)).await;
        assert_eq!(major.unwrap(), 17);
    }

    #[tokio::test]
    async fn a_probe_that_never_answers_says_it_timed_out() {
        let error = probe("#!/bin/sh\nexec sleep 5\n", Duration::from_millis(100))
            .await
            .unwrap_err();
        assert!(matches!(&error, DumpError::ToolVersionUnknown { reason, .. } if reason.contains("did not answer")));
    }

    #[tokio::test]
    async fn a_probe_with_unreadable_output_reports_what_it_printed() {
        let error = probe(
            "#!/bin/sh\necho 'wrapper: no such library' >&2\necho 'hello'\nexit 3\n",
            Duration::from_secs(5),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("exit status: 3"), "{error}");
        assert!(error.contains("hello") && error.contains("no such library"), "{error}");
    }
}
