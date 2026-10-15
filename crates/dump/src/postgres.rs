use std::ffi::OsString;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use secrecy::ExposeSecret;
use tablepro_core::{AuthMode, ConnectOptions, TlsMode};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::passfile::PassFile;
use crate::run::{Finished, run_supervised};
use crate::{ClientTool, DumpError};

const APPLICATION_NAME: &str = "BookiE backup";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DumpFormat {
    Custom,
    Plain,
}

#[derive(Debug)]
pub struct PgDumpRequest<'a> {
    pub options: &'a ConnectOptions,
    pub output: &'a Path,
    pub format: DumpFormat,
    pub server_major: u32,
}

pub async fn pg_dump(
    tool: &ClientTool,
    request: PgDumpRequest<'_>,
    runtime_dir: &Path,
    cancel: CancellationToken,
) -> Result<(), DumpError> {
    tool.require_at_least(request.server_major)?;
    if !request.output.is_absolute() {
        return Err(DumpError::OutputNotAbsolute);
    }
    let mut env = libpq_env(request.options)?;
    let passfile = if request.options.password.expose_secret().is_empty() {
        None
    } else {
        Some(PassFile::create(runtime_dir, &request.options.password)?)
    };
    if let Some(passfile) = &passfile {
        env.push(("PGPASSFILE", passfile.path().as_os_str().to_owned()));
    }
    let partial = partial_path(request.output);
    let mut command = Command::new(&tool.path);
    command.env_clear().envs(env).args(dump_args(request.format, &partial));
    let result = run_supervised(command, cancel).await;
    drop(passfile);
    finish(tool, request.options, result, &partial, request.output)
}

fn finish(
    tool: &ClientTool,
    options: &ConnectOptions,
    result: Result<Finished, DumpError>,
    partial: &Path,
    output: &Path,
) -> Result<(), DumpError> {
    let outcome = result.and_then(|finished| {
        if finished.status.success() {
            return Ok(());
        }
        Err(DumpError::Failed {
            tool: tool.name.clone(),
            status: finished.status.code(),
            stderr: scrub(&finished.stderr, options),
        })
    });
    if let Err(error) = outcome {
        let _ = std::fs::remove_file(partial);
        return Err(error);
    }
    std::fs::rename(partial, output)?;
    Ok(())
}

fn dump_args(format: DumpFormat, partial: &Path) -> Vec<OsString> {
    let format = match format {
        DumpFormat::Custom => "--format=custom",
        DumpFormat::Plain => "--format=plain",
    };
    let mut file = OsString::from("--file=");
    file.push(partial.as_os_str());
    vec!["--no-password".into(), format.into(), file]
}

fn partial_path(output: &Path) -> PathBuf {
    let mut name = output.file_name().map(OsString::from).unwrap_or_default();
    name.push(format!(".partial-{}", uuid::Uuid::new_v4()));
    output.with_file_name(name)
}

pub(crate) fn libpq_env(options: &ConnectOptions) -> Result<Vec<(&'static str, OsString)>, DumpError> {
    if options.auth_mode == AuthMode::Kerberos {
        return Err(DumpError::Unsupported(
            "Kerberos sign-in is not supported for backups yet",
        ));
    }
    if options.forwarded_socket_dir.is_some() {
        return Err(DumpError::Unsupported(
            "the SSH route forwards a socket, which the client tools cannot verify",
        ));
    }
    if options.tls.pinned_fingerprint.is_some() {
        return Err(DumpError::Unsupported(
            "certificate pinning is not available to the client tools",
        ));
    }
    let mut env = vec![
        ("LC_ALL", OsString::from("C")),
        ("PGDATABASE", options.database.clone().into()),
        ("PGUSER", options.username.clone().into()),
        ("PGAPPNAME", APPLICATION_NAME.into()),
    ];
    env.extend(endpoint_env(options));
    env.extend(tls_env(options));
    if let Some(seconds) = options.connect_timeout_secs {
        env.push(("PGCONNECT_TIMEOUT", seconds.to_string().into()));
    }
    Ok(env)
}

fn endpoint_env(options: &ConnectOptions) -> Vec<(&'static str, OsString)> {
    if let Some(dir) = &options.local_socket_dir {
        return vec![
            ("PGHOST", dir.clone().into()),
            ("PGPORT", options.port.to_string().into()),
        ];
    }
    let dial = unbracketed(&options.host);
    let mut env = vec![("PGPORT", options.port.to_string().into())];
    if options.service_endpoint.is_some() && dial.parse::<IpAddr>().is_ok() {
        env.push(("PGHOST", options.service_address().0.into()));
        env.push(("PGHOSTADDR", dial.into()));
    } else {
        env.push(("PGHOST", dial.into()));
    }
    env
}

fn unbracketed(host: &str) -> &str {
    let host = host.trim();
    host.strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(host)
}

fn tls_env(options: &ConnectOptions) -> Vec<(&'static str, OsString)> {
    let tls = &options.tls;
    let mode = match tls.mode {
        TlsMode::Disabled => "disable",
        TlsMode::Prefer => "prefer",
        TlsMode::Require => "require",
        TlsMode::VerifyCa => "verify-ca",
        TlsMode::VerifyFull => "verify-full",
    };
    let mut env = vec![("PGSSLMODE", OsString::from(mode))];
    if tls.mode.verifies_cert() {
        let root = tls
            .root_cert
            .clone()
            .map_or_else(|| OsString::from("system"), OsString::from);
        env.push(("PGSSLROOTCERT", root));
    }
    if let Some(cert) = &tls.client_cert {
        env.push(("PGSSLCERT", cert.clone().into()));
    }
    if let Some(key) = &tls.client_key {
        env.push(("PGSSLKEY", key.clone().into()));
    }
    env
}

fn scrub(stderr: &str, options: &ConnectOptions) -> String {
    let mut details: Vec<&str> = vec![&options.host, &options.username, &options.database];
    if let Some((service, _)) = &options.service_endpoint {
        details.push(service);
    }
    details.retain(|detail| detail.len() > 1);
    details.sort_by_key(|detail| std::cmp::Reverse(detail.len()));
    let mut text = stderr.trim().to_owned();
    for detail in details {
        text = text.replace(detail, "[redacted]");
    }
    text
}

#[cfg(test)]
#[path = "postgres_tests.rs"]
mod tests;
