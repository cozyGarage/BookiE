use std::io::Read;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use bookie_policy::ApprovalOutcome;

pub(crate) const MAX_ANSWER_BYTES: usize = 64;
const POLL_SLICE_MILLIS: u128 = 250;

/// Reads one answer line. Returns `None` when the deadline passes or `stop` is
/// raised, which is checked every poll slice so a request that has ended
/// releases its blocking worker promptly.
pub(crate) fn read_answer<T: Read + AsRawFd>(
    source: &mut T,
    deadline: Instant,
    stop: &AtomicBool,
) -> Option<ApprovalOutcome> {
    let mut answer = Vec::with_capacity(MAX_ANSWER_BYTES);
    while answer.len() < MAX_ANSWER_BYTES {
        if !wait_readable(source.as_raw_fd(), deadline, stop) {
            return None;
        }
        let mut byte = [0u8; 1];
        if !matches!(source.read(&mut byte), Ok(1)) {
            return Some(ApprovalOutcome::Deny);
        }
        if byte[0] == b'\n' {
            return Some(super::outcome_for_answer(&String::from_utf8_lossy(&answer)));
        }
        answer.push(byte[0]);
    }
    Some(ApprovalOutcome::Deny)
}

pub(crate) fn discard_pending_input(terminal: &impl AsRawFd) {
    let _ = flush_input(terminal.as_raw_fd());
}

fn wait_readable(descriptor: RawFd, deadline: Instant, stop: &AtomicBool) -> bool {
    loop {
        if stop.load(Ordering::Acquire) {
            return false;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        let millis = i32::try_from(remaining.as_millis().min(POLL_SLICE_MILLIS))
            .unwrap_or(1)
            .max(1);
        match poll_readable(descriptor, millis) {
            Some(true) => return true,
            Some(false) => {}
            None => return false,
        }
    }
}

fn poll_readable(descriptor: RawFd, millis: i32) -> Option<bool> {
    let mut request = libc::pollfd {
        fd: descriptor,
        events: libc::POLLIN,
        revents: 0,
    };
    let ready = unsafe { libc::poll(&mut request, 1, millis) };
    match ready {
        0 => Some(false),
        count if count > 0 => Some(true),
        _ if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted => Some(false),
        _ => None,
    }
}

fn flush_input(descriptor: RawFd) -> std::io::Result<()> {
    let result = unsafe { libc::tcflush(descriptor, libc::TCIFLUSH) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(test)]
#[path = "tty_answer_tests.rs"]
mod tests;
