use std::path::PathBuf;

use super::{OpenSshError, TimeoutPhase};

const PROTOCOL_DETAIL_LIMIT: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeclinedHostKey {
    pub algorithm: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ClassifyContext<'a> {
    pub host: &'a str,
    pub forward_target: Option<&'a str>,
    pub declined_host_key: Option<&'a DeclinedHostKey>,
}

pub(crate) fn classify(status: Option<i32>, stderr: &str, context: &ClassifyContext<'_>) -> OpenSshError {
    host_key_failure(stderr, context)
        .or_else(|| authentication_failure(stderr))
        .or_else(|| forwarding_failure(stderr, context))
        .or_else(|| configuration_failure(stderr))
        .or_else(|| network_failure(stderr))
        .unwrap_or_else(|| OpenSshError::Protocol {
            detail: protocol_detail(status, stderr),
        })
}

fn host_key_failure(stderr: &str, context: &ClassifyContext<'_>) -> Option<OpenSshError> {
    if let Some(line) = line_containing(stderr, " revoked by file ") {
        return Some(OpenSshError::HostKeyRevoked {
            host: context.host.to_owned(),
            detail: line.to_owned(),
        });
    }
    if stderr.contains("REVOKED HOST KEY DETECTED") {
        let host = between(stderr, " host key for ", " is marked as revoked.").unwrap_or(context.host);
        let detail = line_containing(stderr, "is marked as revoked.").unwrap_or("the host key is revoked");
        return Some(OpenSshError::HostKeyRevoked {
            host: host.to_owned(),
            detail: detail.to_owned(),
        });
    }
    if stderr.contains("REMOTE HOST IDENTIFICATION HAS CHANGED") {
        return Some(changed_host_key(stderr, context));
    }
    if let Some(line) = line_containing(stderr, "host key is known for") {
        let algorithm = between(line, "No ", " host key is known for").unwrap_or_default();
        let host = between(line, " host key is known for ", " and you have").unwrap_or(context.host);
        return Some(unknown_host_key(host, algorithm, "", false));
    }
    if stderr.contains("Exiting, you have requested strict checking.") {
        return Some(unknown_host_key(context.host, "", "", false));
    }
    if stderr.contains("Host key verification failed.") {
        let (algorithm, fingerprint) = context
            .declined_host_key
            .map_or(("", ""), |key| (key.algorithm.as_str(), key.fingerprint.as_str()));
        return Some(unknown_host_key(
            context.host,
            algorithm,
            fingerprint,
            context.declined_host_key.is_some(),
        ));
    }
    None
}

fn changed_host_key(stderr: &str, context: &ClassifyContext<'_>) -> OpenSshError {
    let mut lines = stderr.lines();
    let mut algorithm = "";
    let mut fingerprint = "";
    while let Some(line) = lines.next() {
        if let Some(found) = between(line, "The fingerprint for the ", " key sent by the remote host is") {
            algorithm = found;
            fingerprint = lines.next().map_or("", |next| next.trim().trim_end_matches('.'));
            break;
        }
    }
    let (known_hosts, line) = line_containing(stderr, "Offending ")
        .and_then(|offending| offending.split_once(" key in "))
        .and_then(|(_, location)| location.rsplit_once(':'))
        .map_or((PathBuf::new(), 0), |(path, line)| {
            (PathBuf::from(path), line.trim().parse().unwrap_or(0))
        });
    let host = between(stderr, "Host key for ", " has changed").unwrap_or(context.host);
    OpenSshError::HostKeyChanged {
        host: host.to_owned(),
        algorithm: algorithm.to_owned(),
        fingerprint: fingerprint.to_owned(),
        known_hosts,
        line,
    }
}

fn unknown_host_key(host: &str, algorithm: &str, fingerprint: &str, declined: bool) -> OpenSshError {
    OpenSshError::HostKeyUnknown {
        host: host.to_owned(),
        algorithm: algorithm.to_owned(),
        fingerprint: fingerprint.to_owned(),
        declined,
    }
}

fn authentication_failure(stderr: &str) -> Option<OpenSshError> {
    let line = line_containing(stderr, ": Permission denied (")?;
    let (user_host, methods) = line.split_once(": Permission denied (")?;
    let (user, host) = user_host.rsplit_once('@')?;
    let methods = methods.trim_end_matches(").").split(',').map(str::to_owned).collect();
    Some(OpenSshError::AuthenticationRejected {
        user: user.trim().to_owned(),
        host: host.to_owned(),
        methods,
    })
}

fn forwarding_failure(stderr: &str, context: &ClassifyContext<'_>) -> Option<OpenSshError> {
    let target = context.forward_target.unwrap_or_default().to_owned();
    if let Some(line) = line_containing(stderr, "master forward request failed") {
        return Some(OpenSshError::ForwardRejected {
            target,
            detail: line.to_owned(),
        });
    }
    let line = stderr
        .lines()
        .find(|line| line.contains("channel ") && line.contains(": open failed"))?;
    Some(OpenSshError::ChannelOpenFailed {
        target,
        detail: line.trim().to_owned(),
    })
}

fn configuration_failure(stderr: &str) -> Option<OpenSshError> {
    if let Some(path) = between(stderr, "ControlPath too long ('", "' >= ") {
        return Some(OpenSshError::ControlPathTooLong {
            path: PathBuf::from(path),
        });
    }
    let line = line_containing(stderr, "Bad configuration option")
        .or_else(|| line_containing(stderr, "Bad owner or permissions"))?;
    Some(OpenSshError::ConfigRejected {
        detail: line.to_owned(),
    })
}

fn network_failure(stderr: &str) -> Option<OpenSshError> {
    if let Some(line) = line_containing(stderr, "Could not resolve hostname ") {
        let rest = line.split_once("Could not resolve hostname ")?.1;
        let (host, detail) = rest.split_once(": ").unwrap_or((rest, ""));
        return Some(OpenSshError::NameResolution {
            host: host.to_owned(),
            detail: detail.to_owned(),
        });
    }
    let line = line_containing(stderr, "connect to host ")?;
    let rest = line.split_once("connect to host ")?.1;
    let (host, rest) = rest.split_once(" port ")?;
    let (port, detail) = rest.split_once(": ")?;
    if detail.contains("timed out") {
        return Some(OpenSshError::Timeout {
            phase: TimeoutPhase::Handshake,
        });
    }
    let port = port.parse().ok()?;
    let host = host.to_owned();
    if detail.contains("Connection refused") {
        return Some(OpenSshError::Refused { host, port });
    }
    Some(OpenSshError::Unreachable {
        host,
        port,
        detail: detail.to_owned(),
    })
}

fn protocol_detail(status: Option<i32>, stderr: &str) -> String {
    let trimmed = stderr.trim();
    let mut start = trimmed.len().saturating_sub(PROTOCOL_DETAIL_LIMIT);
    while !trimmed.is_char_boundary(start) {
        start += 1;
    }
    let tail = &trimmed[start..];
    match status {
        Some(code) => format!("ssh exited with status {code}: {tail}"),
        None => format!("ssh ended without a status: {tail}"),
    }
}

fn line_containing<'a>(text: &'a str, needle: &str) -> Option<&'a str> {
    text.lines().find(|line| line.contains(needle)).map(str::trim)
}

fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let after = text.split_once(start)?.1;
    after.split_once(end).map(|(inner, _)| inner)
}

#[cfg(test)]
#[path = "stderr_classify_tests.rs"]
mod tests;
