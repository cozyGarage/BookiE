#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tablepro_ssh::openssh::{
    OpenSshAuth, OpenSshConfig, OpenSshContext, OpenSshRuntime, OpenSshSession, OpenSshTimeouts, SshDestination,
    UnattendedPrompter,
};
use tokio_util::sync::CancellationToken;

const TMP_DIR_ENV: &str = "TABLEPRO_TEST_PDEATHSIG_DIR";

const FAKE_SSH_THAT_NEVER_EXITS: &str = r#"#!/bin/sh
dir='@DIR@'
case " $* " in
  *" -O "*) exit 0 ;;
esac
echo "$$" > "$dir/master.pid"
exec sleep 300
"#;

fn process_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn wait_for(timeout: Duration, mut check: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    loop {
        if check() {
            return true;
        }
        if start.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn run_harness(dir: PathBuf) -> ! {
    let script = dir.join("ssh");
    std::fs::write(
        &script,
        FAKE_SSH_THAT_NEVER_EXITS.replace("@DIR@", &dir.display().to_string()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let base = dir.join("run");
    std::fs::create_dir(&base).unwrap();
    let context = OpenSshContext {
        runtime: OpenSshRuntime::acquire(&base).unwrap(),
        ssh_program: script,
        askpass_program: dir.join("askpass"),
        timeouts: OpenSshTimeouts {
            handshake: Duration::from_secs(60),
            ..Default::default()
        },
    };
    let config = OpenSshConfig {
        destination: SshDestination::new("bastion", None, None).unwrap(),
        jump_hosts: Vec::new(),
        auth: OpenSshAuth::Agent,
    };
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.spawn(async move {
        let _ = OpenSshSession::connect(
            &config,
            &context,
            Arc::new(UnattendedPrompter),
            CancellationToken::new(),
        )
        .await;
    });
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

#[test]
fn killing_the_parent_process_takes_the_ssh_master_down_with_it() {
    if let Ok(dir) = std::env::var(TMP_DIR_ENV) {
        run_harness(PathBuf::from(dir));
    }

    let dir = tempfile::tempdir().unwrap();
    let exe = std::env::current_exe().unwrap();
    let mut harness = Command::new(&exe)
        .arg("killing_the_parent_process_takes_the_ssh_master_down_with_it")
        .arg("--exact")
        .env(TMP_DIR_ENV, dir.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the harness process must spawn");

    let pid_file = dir.path().join("master.pid");
    assert!(
        wait_for(Duration::from_secs(10), || pid_file.exists()),
        "the harness never spawned its ssh master"
    );
    let master_pid: u32 = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .expect("the fake ssh master must record its pid");
    assert!(
        process_alive(master_pid),
        "the ssh master must be running before the parent is killed"
    );

    harness.kill().expect("the harness process must be killable");
    harness.wait().expect("the harness process must be reaped");

    assert!(
        wait_for(Duration::from_secs(10), || !process_alive(master_pid)),
        "the ssh master must die when its parent is killed"
    );
}
