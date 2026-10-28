#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;

#[test]
#[ignore = "requires an isolated GTK display"]
fn information_flags_exit_without_display_or_profile_access() {
    for (flag, expected) in [
        ("--version", format!("BookiE {}", env!("CARGO_PKG_VERSION"))),
        ("--help", "Usage: bookie [--help] [--version]".to_owned()),
    ] {
        let home = tempfile::tempdir().unwrap();
        let output = Command::new(
            std::env::var_os("CARGO_BIN_EXE_tablepro-app").unwrap_or_else(|| env!("CARGO_BIN_EXE_tablepro-app").into()),
        )
        .arg(flag)
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("XDG_CACHE_HOME", home.path().join("cache"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
        assert!(
            output.status.success(),
            "{flag}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), expected);
        assert!(home.path().read_dir().unwrap().next().is_none());
    }
}
