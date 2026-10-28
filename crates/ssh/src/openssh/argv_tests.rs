use std::path::PathBuf;

use secrecy::SecretString;

use super::*;
use crate::openssh::SshDestination;

fn config(port: Option<u16>, user: Option<&str>, auth: OpenSshAuth) -> OpenSshConfig {
    OpenSshConfig {
        destination: SshDestination::new("db-bastion", port, user.map(str::to_owned)).unwrap(),
        jump_hosts: Vec::new(),
        auth,
    }
}

fn strings(args: &[OsString]) -> Vec<&str> {
    args.iter().map(|arg| arg.to_str().unwrap()).collect()
}

fn master(config: &OpenSshConfig) -> Vec<OsString> {
    master_args(
        config,
        Path::new("/tmp/ssh_config"),
        Path::new("/tmp/control"),
        &OpenSshTimeouts::default(),
    )
}

#[test]
fn master_argv_is_exact() {
    let args = master_args(
        &config(None, None, OpenSshAuth::Agent),
        Path::new("/tmp/ssh_config"),
        Path::new("/run/user/1000/tablepro/ssh/0123456789abcdef/89abcdef/control"),
        &OpenSshTimeouts::default(),
    );
    assert_eq!(
        strings(&args),
        [
            "-M",
            "-N",
            "-T",
            "-F",
            "/tmp/ssh_config",
            "-o",
            "ControlPath=/run/user/1000/tablepro/ssh/0123456789abcdef/89abcdef/control",
            "-o",
            "ControlMaster=yes",
            "-o",
            "ControlPersist=no",
            "-o",
            "ForkAfterAuthentication=no",
            "-o",
            "StreamLocalBindMask=0177",
            "-o",
            "StreamLocalBindUnlink=no",
            "-o",
            "BatchMode=no",
            "-o",
            "StrictHostKeyChecking=ask",
            "-o",
            "VisualHostKey=no",
            "-o",
            "Tunnel=no",
            "-o",
            "ClearAllForwardings=yes",
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "ForwardAgent=no",
            "-o",
            "ForwardX11=no",
            "-o",
            "PermitLocalCommand=no",
            "-o",
            "RequestTTY=no",
            "-o",
            "SessionType=none",
            "-o",
            "GatewayPorts=no",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ServerAliveCountMax=3",
            "-o",
            "LogLevel=INFO",
            "--",
            "db-bastion",
        ]
    );
}

#[test]
fn every_forced_option_precedes_the_destination_and_the_destination_follows_the_separator() {
    let mut with_everything = config(Some(2222), Some("deploy"), OpenSshAuth::Agent);
    with_everything.jump_hosts = SshDestination::parse_jump_list("ops@jump1").unwrap();
    let args = master(&with_everything);
    let args = strings(&args);
    let separator = args.iter().position(|arg| *arg == "--").unwrap();
    assert_eq!(args[separator + 1..], ["db-bastion"]);
    for option in FORCED_OPTIONS {
        let position = args.iter().position(|arg| *arg == option).unwrap();
        assert!(position < separator, "{option}");
        assert_eq!(args[position - 1], "-o");
    }
}

#[test]
fn port_and_user_only_when_set() {
    let without = strings(&master(&config(None, None, OpenSshAuth::Agent))).join(" ");
    assert!(!without.contains(" -p ") && !without.contains(" -l "));
    let with = master(&config(Some(2222), Some("deploy"), OpenSshAuth::Agent));
    assert_eq!(
        strings(&with[with.len() - 6..]),
        ["-p", "2222", "-l", "deploy", "--", "db-bastion"]
    );
}

#[test]
fn explicit_jump_hosts_use_openssh_proxyjump() {
    let mut with_jumps = config(None, None, OpenSshAuth::Agent);
    with_jumps.jump_hosts = SshDestination::parse_jump_list("ops@jump1:2200,jump2").unwrap();
    let args = master(&with_jumps);
    let args = strings(&args);
    assert!(args.windows(2).any(|pair| pair == ["-J", "ops@jump1:2200,jump2"]));
}

#[test]
fn private_key_path_adds_minus_i() {
    let auth = OpenSshAuth::PrivateKey {
        path: Some(PathBuf::from("/home/deploy/.ssh/id_ed25519")),
        passphrase: None,
    };
    let joined = strings(&master(&config(None, None, auth))).join(" ");
    assert!(
        joined.ends_with(" -i /home/deploy/.ssh/id_ed25519 -- db-bastion"),
        "{joined}"
    );

    let without_path = OpenSshAuth::PrivateKey {
        path: None,
        passphrase: None,
    };
    assert!(!strings(&master(&config(None, None, without_path))).contains(&"-i"));
}

#[test]
fn preferred_authentications_per_mode() {
    let cases = [
        (OpenSshAuth::Agent, None),
        (
            OpenSshAuth::Password {
                password: SecretString::from("secret"),
            },
            Some("PreferredAuthentications=password,keyboard-interactive,publickey"),
        ),
    ];
    for (auth, expected) in cases {
        let args = master(&config(None, None, auth));
        let preferred = strings(&args)
            .into_iter()
            .find(|arg| arg.starts_with("PreferredAuthentications="));
        assert_eq!(preferred, expected);
    }
}

#[test]
fn the_password_never_reaches_the_command_line() {
    let auth = OpenSshAuth::Password {
        password: SecretString::from("s3cret-value"),
    };
    let args = master(&config(None, Some("deploy"), auth));
    assert!(strings(&args).iter().all(|arg| !arg.contains("s3cret-value")));
}

#[test]
fn control_commands_ignore_the_user_config_and_escape_percent() {
    let target = ForwardTarget::new("fd00::5", 5432).unwrap();
    let unix = LocalEndpoint::Unix(PathBuf::from("/run/user/1000/m/f1"));
    let tcp = LocalEndpoint::Tcp("127.0.0.1:40123".parse().unwrap());
    let control = Path::new("/run/user/1000/m/control%");
    let ops = [
        ControlOp::Check,
        ControlOp::Forward {
            listen: &unix,
            target: &target,
        },
        ControlOp::Cancel {
            listen: &tcp,
            target: &target,
        },
        ControlOp::Exit,
    ];
    for op in ops {
        let args = control_args(control, op);
        let args = strings(&args);
        assert_eq!(
            args[..6],
            [
                "-F",
                "none",
                "-o",
                "ControlPath=/run/user/1000/m/control%%",
                "-o",
                "LogLevel=INFO"
            ]
        );
        assert_eq!(args[args.len() - 2..], ["--", "tablepro"]);
    }
    let forward = control_args(
        control,
        ControlOp::Forward {
            listen: &unix,
            target: &target,
        },
    );
    assert_eq!(
        strings(&forward)[6..10],
        ["-O", "forward", "-L", "/run/user/1000/m/f1:[fd00::5]:5432"]
    );
    let cancel = control_args(
        control,
        ControlOp::Cancel {
            listen: &tcp,
            target: &target,
        },
    );
    assert_eq!(
        strings(&cancel)[6..10],
        ["-O", "cancel", "-L", "127.0.0.1:40123:[fd00::5]:5432"]
    );
    assert_eq!(strings(&control_args(control, ControlOp::Exit))[6..8], ["-O", "exit"]);
}
