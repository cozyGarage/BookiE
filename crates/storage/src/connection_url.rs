use secrecy::SecretString;
use uuid::Uuid;

use tablepro_core::{AuthMode, Environment, TlsMode};

use crate::connections::SavedConnection;
use crate::error::StorageError;

/// Longest URL accepted. A connection URL is pasted by hand or arrives
/// from a clipboard, so the bound exists to stop a pathological input
/// from driving allocation in the percent decoder.
const MAX_URL_LEN: usize = 2048;
const MAX_HOST_LEN: usize = 253;
const MAX_FIELD_LEN: usize = 256;

/// Scheme aliases mapped to the driver id they select and the port to
/// assume when the URL omits one. The app crate has a test that pins
/// each port against the registered driver's own default.
const SCHEMES: &[(&str, &str, u16)] = &[
    ("postgres", "postgres", 5432),
    ("postgresql", "postgres", 5432),
    ("mysql", "mysql", 3306),
    ("mariadb", "mysql", 3306),
    ("mssql", "mssql", 1433),
    ("sqlserver", "mssql", 1433),
    ("clickhouse", "clickhouse", 8123),
    ("mongodb", "mongodb", 27017),
    ("redis", "redis", 6379),
];

/// A connection URL after parsing. The password never reaches
/// `connection`: it stays in a `secrecy` wrapper so the caller has to
/// hand it to Secret Service explicitly, and a caller that only saves
/// the JSON record cannot leak it to disk.
pub struct ParsedConnectionUrl {
    pub connection: SavedConnection,
    pub password: Option<SecretString>,
}

pub fn parse_connection_url(input: &str) -> Result<ParsedConnectionUrl, StorageError> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(invalid("a connection URL is required"));
    }
    if raw.len() > MAX_URL_LEN {
        return Err(invalid("the connection URL is too long"));
    }
    if raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(invalid("the connection URL contains whitespace or control characters"));
    }

    let Some((scheme, remainder)) = raw.split_once("://") else {
        return Err(invalid("the connection URL needs a scheme such as postgres://"));
    };
    let scheme = scheme.to_ascii_lowercase();
    let Some(&(_, driver_id, default_port)) = SCHEMES.iter().find(|(alias, _, _)| *alias == scheme) else {
        return Err(invalid(&format!("{scheme} is not a database scheme TablePro knows")));
    };

    if remainder.contains('#') {
        return Err(invalid("connection URL fragments are not supported"));
    }
    let (remainder, query) = remainder.split_once('?').unwrap_or((remainder, ""));
    let option = query_option(driver_id, query)?;
    let tls_mode = match option {
        Some(UrlOption::Tls(mode)) => mode,
        _ => TlsMode::Disabled,
    };
    let socket = match option {
        Some(UrlOption::Socket(path)) => Some(path),
        _ => None,
    };
    let (authority, path) = match remainder.split_once('/') {
        Some((authority, path)) => (authority, path),
        None => (remainder, ""),
    };
    if path.contains('/') {
        return Err(invalid("the connection URL has more than one path segment"));
    }

    let (credentials, host_port) = match authority.rsplit_once('@') {
        Some((credentials, host_port)) => (Some(credentials), host_port),
        None => (None, authority),
    };

    let (username, password) = match credentials {
        None => (String::new(), None),
        Some(credentials) => {
            let (user, secret) = match credentials.split_once(':') {
                Some((user, secret)) => (user, Some(secret)),
                None => (credentials, None),
            };
            let user = decode_field(user, "username")?;
            let secret = match secret {
                Some(secret) if !secret.is_empty() => Some(SecretString::from(decode_field(secret, "password")?)),
                _ => None,
            };
            (user, secret)
        }
    };

    let (host, port) = match &socket {
        Some(_) => socket_host_port(host_port, default_port)?,
        None => split_host_port(host_port, default_port)?,
    };
    let database = decode_field(path, "database")?;

    let place = socket
        .as_ref()
        .map_or_else(|| host.clone(), |path| path.display().to_string());
    let name = if database.is_empty() {
        place
    } else {
        format!("{database} on {place}")
    };
    let (socket_dir, socket_path) = match socket {
        Some(path) if driver_id == "mysql" => (None, Some(path)),
        Some(path) => (Some(path), None),
        None => (None, None),
    };

    let connection = SavedConnection {
        auth_source: None,
        id: Uuid::new_v4(),
        name,
        driver_id: driver_id.to_string(),
        host,
        port,
        socket_dir,
        socket_path,
        database,
        username,
        use_tls: tls_mode.encrypts(),
        tls_mode: Some(tls_mode),
        tls_root_cert: None,
        tls_client_cert: None,
        tls_client_key: None,
        read_only: false,
        auth_mode: AuthMode::Password,
        environment: Environment::Local,
        ssh: None,
        last_opened_at: None,
        connect_timeout_secs: None,
        query_timeout_secs: None,
    };

    Ok(ParsedConnectionUrl { connection, password })
}

enum UrlOption {
    Tls(TlsMode),
    Socket(std::path::PathBuf),
}

fn query_option(driver: &str, query: &str) -> Result<Option<UrlOption>, StorageError> {
    if query.is_empty() {
        return Ok(None);
    }
    if query.contains('&') {
        return Err(invalid(
            "duplicate or additional connection URL options are not supported",
        ));
    }
    let (key, value) = query
        .split_once('=')
        .ok_or_else(|| invalid("connection URL options require a name and value"))?;
    let key = decode_field(key, "option name")?;
    let value = decode_field(value, "option value")?;
    if matches!((driver, key.as_str()), ("mysql", "socket") | ("postgres", "host")) {
        return local_socket(&value).map(|path| Some(UrlOption::Socket(path)));
    }
    let mode = match (driver, key.as_str(), value.as_str()) {
        ("postgres", "sslmode", "disable") | ("mysql", "ssl-mode", "DISABLED") | ("mongodb", "tls", "false") => {
            TlsMode::Disabled
        }
        ("postgres", "sslmode", "prefer") | ("mysql", "ssl-mode", "PREFERRED") => TlsMode::Prefer,
        ("postgres", "sslmode", "require") | ("mysql", "ssl-mode", "REQUIRED") => TlsMode::Require,
        ("postgres", "sslmode", "verify-ca") | ("mysql", "ssl-mode", "VERIFY_CA") => TlsMode::VerifyCa,
        ("postgres", "sslmode", "verify-full")
        | ("mysql", "ssl-mode", "VERIFY_IDENTITY")
        | ("mongodb", "tls", "true") => TlsMode::VerifyFull,
        _ => {
            return Err(invalid(
                "unsupported connection URL option; configure this connection in the form",
            ));
        }
    };
    Ok(Some(UrlOption::Tls(mode)))
}

fn local_socket(value: &str) -> Result<std::path::PathBuf, StorageError> {
    let path = std::path::Path::new(value);
    let plain = path.components().all(|component| {
        !matches!(
            component,
            std::path::Component::ParentDir | std::path::Component::CurDir
        )
    });
    if !path.is_absolute() || !plain || path.parent().is_none() {
        return Err(invalid("a local socket in a connection URL must be an absolute path"));
    }
    Ok(path.to_path_buf())
}

fn socket_host_port(authority: &str, default_port: u16) -> Result<(String, u16), StorageError> {
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, parse_port(Some(port), default_port)?),
        None => (authority, default_port),
    };
    if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
        return Err(invalid(
            "a connection URL with a local socket must not name a network host",
        ));
    }
    Ok(("localhost".into(), port))
}

fn split_host_port(authority: &str, default_port: u16) -> Result<(String, u16), StorageError> {
    if authority.is_empty() {
        return Err(invalid("the connection URL is missing a host"));
    }
    // Bracketed IPv6 literal: the colons inside the brackets are part of
    // the address, so only a colon after ']' introduces a port.
    if let Some(rest) = authority.strip_prefix('[') {
        let Some((address, tail)) = rest.split_once(']') else {
            return Err(invalid("the bracketed host in the connection URL is not closed"));
        };
        let port = parse_port(tail.strip_prefix(':'), default_port)?;
        return Ok((validate_host(address)?, port));
    }
    match authority.rsplit_once(':') {
        Some((host, port)) => Ok((validate_host(host)?, parse_port(Some(port), default_port)?)),
        None => Ok((validate_host(authority)?, default_port)),
    }
}

fn parse_port(port: Option<&str>, default_port: u16) -> Result<u16, StorageError> {
    let Some(port) = port else {
        return Ok(default_port);
    };
    if port.is_empty() {
        return Ok(default_port);
    }
    let parsed: u16 = port
        .parse()
        .map_err(|_| invalid("the port in the connection URL is not a number between 1 and 65535"))?;
    if parsed == 0 {
        return Err(invalid(
            "the port in the connection URL is not a number between 1 and 65535",
        ));
    }
    Ok(parsed)
}

fn validate_host(host: &str) -> Result<String, StorageError> {
    if host.is_empty() {
        return Err(invalid("the connection URL is missing a host"));
    }
    if host.len() > MAX_HOST_LEN {
        return Err(invalid("the host in the connection URL is too long"));
    }
    let acceptable = host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '%'));
    if !acceptable {
        return Err(invalid(
            "the host in the connection URL contains characters that are not allowed",
        ));
    }
    Ok(host.to_ascii_lowercase())
}

fn decode_field(raw: &str, field: &str) -> Result<String, StorageError> {
    if raw.len() > MAX_FIELD_LEN {
        return Err(invalid(&format!("the {field} in the connection URL is too long")));
    }
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            out.push(bytes[index]);
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return Err(invalid(&format!(
                "the {field} in the connection URL has a truncated escape"
            )));
        }
        let high = hex_value(bytes[index + 1]);
        let low = hex_value(bytes[index + 2]);
        match (high, low) {
            (Some(high), Some(low)) => out.push(high * 16 + low),
            _ => {
                return Err(invalid(&format!(
                    "the {field} in the connection URL has an invalid escape"
                )));
            }
        }
        index += 3;
    }
    let decoded = String::from_utf8(out)
        .map_err(|_| invalid(&format!("the {field} in the connection URL is not valid UTF-8")))?;
    if decoded.chars().any(char::is_control) {
        return Err(invalid(&format!(
            "the {field} in the connection URL contains control characters"
        )));
    }
    Ok(decoded)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn invalid(message: &str) -> StorageError {
    StorageError::Schema(message.to_string())
}

#[cfg(test)]
#[path = "connection_url_tests.rs"]
mod tests;
