use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui::{Entity, VisualTestContext};

use crate::GridView;
use crate::sidebar_state::SidebarState;

pub(crate) const NARROW: f32 = SidebarState::NARROW_WINDOW - 60.0;
pub(crate) const MEDIUM: f32 = 900.0;
pub(crate) const WIDE: f32 = 1500.0;

const DEADLINE: Duration = Duration::from_secs(15);
const CLOCK_STEP: Duration = Duration::from_millis(100);
const REAL_STEP: Duration = Duration::from_millis(10);

pub(crate) fn until(
    view: &Entity<GridView>,
    cx: &mut VisualTestContext,
    what: &str,
    ready: impl Fn(&GridView) -> bool,
) {
    let deadline = Instant::now() + DEADLINE;
    loop {
        cx.executor().advance_clock(CLOCK_STEP);
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        if view.read_with(cx, |view, _| ready(view)) {
            return;
        }
        if Instant::now() >= deadline {
            let status = view.read_with(cx, |view, _| view.status.clone());
            panic!("{what} did not happen within {DEADLINE:?}; status: {status}");
        }
        std::thread::sleep(REAL_STEP);
    }
}

pub(crate) struct Orders {
    _directory: tempfile::TempDir,
    path: PathBuf,
}

impl std::ops::Deref for Orders {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for Orders {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

pub(crate) fn orders() -> Orders {
    let directory = tempfile::tempdir().expect("a private test directory");
    let path = directory.path().join("orders.sqlite");
    std::fs::write(&path, include_bytes!("../../tests/fixtures/orders.sqlite")).expect("write the orders fixture");
    Orders {
        _directory: directory,
        path,
    }
}

const _: () = {
    use bookie_gpui::theme::size::{COMPACT_QUERY_WIDTH, STATUS_COMPACT_WIDTH};
    assert!(NARROW < SidebarState::NARROW_WINDOW && SidebarState::NARROW_WINDOW < MEDIUM);
    assert!(MEDIUM < STATUS_COMPACT_WIDTH && COMPACT_QUERY_WIDTH < WIDE);
};

#[test]
fn an_orders_copy_is_removed_with_its_guard() {
    let copy = orders();
    let path = copy.to_path_buf();
    assert!(path.is_file());
    drop(copy);
    assert!(!path.exists());
}
