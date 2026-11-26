use std::path::Path;

use super::new_window_command;

#[test]
fn the_new_window_runs_the_same_program_and_does_not_inherit_the_launch_target() {
    let command = new_window_command(Path::new("/usr/bin/bookie-gpui"));
    assert_eq!(command.get_program(), "/usr/bin/bookie-gpui");
    assert_eq!(command.get_args().count(), 0);
    let cleared: Vec<String> = command
        .get_envs()
        .filter(|(_, value)| value.is_none())
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    for name in [
        "BOOKIE_PROTOTYPE_FILE",
        "BOOKIE_PROTOTYPE_CONNECTION",
        "BOOKIE_PROTOTYPE_TABLE",
        "BOOKIE_PROTOTYPE_START",
    ] {
        assert!(
            cleared.iter().any(|cleared| cleared == name),
            "{name} is not cleared: {cleared:?}"
        );
    }
}
