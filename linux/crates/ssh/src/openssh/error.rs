use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutPhase {
    Handshake,
    ChannelOpen,
    Connect,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OpenSshError {
    #[error("the SSH destination is not valid: {detail}")]
    InvalidDestination { detail: String },
    #[error("the SSH forward target is not valid: {detail}")]
    InvalidForwardTarget { detail: String },
    #[error("the SSH runtime directory {path} is not safe to use: {detail}")]
    RuntimeDirUnsafe { path: PathBuf, detail: String },
    #[error("the SSH control socket path {path} is too long for a Unix socket")]
    ControlPathTooLong { path: PathBuf },
    #[error("the OpenSSH client {program} was not found")]
    ClientMissing { program: PathBuf },
    #[error("could not run {program}: {detail}")]
    Spawn { program: PathBuf, detail: String },
    #[error("OpenSSH rejected its configuration: {detail}")]
    ConfigRejected { detail: String },
    #[error("the host key for {host} is not known")]
    HostKeyUnknown {
        host: String,
        algorithm: String,
        fingerprint: String,
        declined: bool,
    },
    #[error("the host key for {host} has changed (see {known_hosts}, line {line})")]
    HostKeyChanged {
        host: String,
        algorithm: String,
        fingerprint: String,
        known_hosts: PathBuf,
        line: usize,
    },
    #[error("the host key for {host} is revoked")]
    HostKeyRevoked { host: String, detail: String },
    #[error("the SSH server {host} rejected authentication for {user}")]
    AuthenticationRejected {
        user: String,
        host: String,
        methods: Vec<String>,
    },
    #[error("the SSH server refused to forward to {target}")]
    ForwardRejected { target: String, detail: String },
    #[error("the SSH server could not open a channel to {target}")]
    ChannelOpenFailed { target: String, detail: String },
    #[error("could not resolve the SSH host {host}")]
    NameResolution { host: String, detail: String },
    #[error("the SSH host {host} refused the connection on port {port}")]
    Refused { host: String, port: u16 },
    #[error("the SSH host {host} on port {port} is unreachable")]
    Unreachable { host: String, port: u16, detail: String },
    #[error("the SSH operation timed out during {phase:?}")]
    Timeout { phase: TimeoutPhase },
    #[error("the SSH connection was cancelled")]
    Cancelled,
    #[error("the SSH master process exited")]
    MasterExited { status: Option<i32>, detail: String },
    #[error("OpenSSH failed: {detail}")]
    Protocol { detail: String },
}
