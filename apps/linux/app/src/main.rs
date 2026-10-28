use std::io::Write;

fn main() {
    match std::env::args_os().nth(1).as_deref().and_then(std::ffi::OsStr::to_str) {
        Some("--version" | "-V") => {
            let _ = writeln!(std::io::stdout().lock(), "BookiE {}", env!("CARGO_PKG_VERSION"));
        }
        Some("--help" | "-h") => {
            let _ = writeln!(std::io::stdout().lock(), "Usage: bookie [--help] [--version] [FILE|URI...]");
        }
        _ => tablepro_app::run(),
    }
}
