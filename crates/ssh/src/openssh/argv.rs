use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;

use super::{ForwardTarget, LocalEndpoint, OpenSshAuth, OpenSshConfig, OpenSshTimeouts};

const FORCED_OPTIONS: [&str; 17] = [
    "ControlMaster=yes",
    "ControlPersist=no",
    "ForkAfterAuthentication=no",
    "StreamLocalBindMask=0177",
    "StreamLocalBindUnlink=no",
    "BatchMode=no",
    "StrictHostKeyChecking=ask",
    "VisualHostKey=no",
    "Tunnel=no",
    "ClearAllForwardings=yes",
    "ExitOnForwardFailure=yes",
    "ForwardAgent=no",
    "ForwardX11=no",
    "PermitLocalCommand=no",
    "RequestTTY=no",
    "SessionType=none",
    "GatewayPorts=no",
];

const CONTROL_HOST: &str = "tablepro";

#[derive(Debug, Clone, Copy)]
pub(crate) enum ControlOp<'a> {
    Check,
    Forward {
        listen: &'a LocalEndpoint,
        target: &'a ForwardTarget,
    },
    Cancel {
        listen: &'a LocalEndpoint,
        target: &'a ForwardTarget,
    },
    Exit,
}

pub(crate) fn master_args(
    config: &OpenSshConfig,
    ssh_config: &Path,
    control: &Path,
    timeouts: &OpenSshTimeouts,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["-M", "-N", "-T"].into_iter().map(OsString::from).collect();
    args.extend(["-F".into(), ssh_config.as_os_str().to_owned()]);
    push_option(&mut args, control_path_option(control));
    for option in FORCED_OPTIONS {
        push_option(&mut args, option.into());
    }
    push_option(
        &mut args,
        format!("ConnectTimeout={}", timeouts.connect.as_secs().max(1)).into(),
    );
    push_option(
        &mut args,
        format!("ServerAliveInterval={}", timeouts.keepalive_interval.as_secs()).into(),
    );
    push_option(
        &mut args,
        format!("ServerAliveCountMax={}", timeouts.keepalive_max).into(),
    );
    push_option(&mut args, "LogLevel=INFO".into());
    push_jump_hosts(&mut args, config);
    push_auth(&mut args, &config.auth);
    if let Some(port) = config.destination.port() {
        args.push("-p".into());
        args.push(port.to_string().into());
    }
    if let Some(user) = config.destination.user() {
        args.push("-l".into());
        args.push(user.into());
    }
    args.push("--".into());
    args.push(config.destination.host().into());
    args
}

fn push_jump_hosts(args: &mut Vec<OsString>, config: &OpenSshConfig) {
    if config.jump_hosts.is_empty() {
        return;
    }
    let hops: Vec<String> = config.jump_hosts.iter().map(ToString::to_string).collect();
    args.extend(["-J".into(), hops.join(",").into()]);
}

fn push_auth(args: &mut Vec<OsString>, auth: &OpenSshAuth) {
    match auth {
        OpenSshAuth::Agent => {}
        OpenSshAuth::PrivateKey { path, .. } => {
            if let Some(path) = path {
                args.push("-i".into());
                args.push(path.as_os_str().to_owned());
            }
        }
        OpenSshAuth::Password { .. } => push_option(
            args,
            "PreferredAuthentications=password,keyboard-interactive,publickey".into(),
        ),
    }
}

pub(crate) fn control_args(control: &Path, op: ControlOp<'_>) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["-F".into(), "none".into()];
    push_option(&mut args, control_path_option(control));
    push_option(&mut args, "LogLevel=INFO".into());
    let (name, forward) = match op {
        ControlOp::Check => ("check", None),
        ControlOp::Forward { listen, target } => ("forward", Some((listen, target))),
        ControlOp::Cancel { listen, target } => ("cancel", Some((listen, target))),
        ControlOp::Exit => ("exit", None),
    };
    args.push("-O".into());
    args.push(name.into());
    if let Some((listen, target)) = forward {
        args.push("-L".into());
        args.push(forward_spec(listen, target));
    }
    args.push("--".into());
    args.push(CONTROL_HOST.into());
    args
}

fn forward_spec(listen: &LocalEndpoint, target: &ForwardTarget) -> OsString {
    let mut spec = match listen {
        LocalEndpoint::Unix(path) => path.as_os_str().to_owned(),
        LocalEndpoint::Tcp(address) => OsString::from(address.to_string()),
    };
    spec.push(format!(":{target}"));
    spec
}

fn push_option(args: &mut Vec<OsString>, option: OsString) {
    args.push("-o".into());
    args.push(option);
}

fn control_path_option(control: &Path) -> OsString {
    let mut bytes = b"ControlPath=".to_vec();
    for &byte in control.as_os_str().as_bytes() {
        if byte == b'%' {
            bytes.push(b'%');
        }
        bytes.push(byte);
    }
    OsString::from_vec(bytes)
}

#[cfg(test)]
#[path = "argv_tests.rs"]
mod tests;
