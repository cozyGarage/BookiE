use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

static CONTAINER_IDS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static CLEANUP_HOOK: OnceLock<()> = OnceLock::new();

pub fn register(container_id: &str) {
    CONTAINER_IDS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap()
        .push(container_id.to_owned());
    if CLEANUP_HOOK.set(()).is_ok() {
        assert_eq!(unsafe { libc::atexit(cleanup) }, 0, "register Docker cleanup");
    }
}

extern "C" fn cleanup() {
    let Some(ids) = CONTAINER_IDS.get() else {
        return;
    };
    let ids = ids.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    for id in ids.iter() {
        let _ = Command::new("docker")
            .args(["rm", "--force", id])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
