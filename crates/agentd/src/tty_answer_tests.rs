use super::*;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::Duration;

fn soon(milliseconds: u64) -> Instant {
    Instant::now() + Duration::from_millis(milliseconds)
}

#[test]
fn silence_until_the_deadline_is_a_timeout_that_returns_promptly() {
    let (mut ours, _theirs) = UnixStream::pair().unwrap();
    let started = Instant::now();
    assert_eq!(read_answer(&mut ours, soon(80), &|| false, "8b7d"), None);
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(70) && elapsed < Duration::from_secs(2),
        "{elapsed:?}"
    );
}

#[test]
fn only_the_matching_prompt_answer_allows() {
    for (typed, expected) in [
        ("approve 8b7d\n", ApprovalOutcome::AllowOnce),
        ("approve other\n", ApprovalOutcome::Deny),
        ("n\n", ApprovalOutcome::Deny),
        ("\n", ApprovalOutcome::Deny),
        ("yes\n", ApprovalOutcome::Deny),
    ] {
        let (mut ours, mut theirs) = UnixStream::pair().unwrap();
        theirs.write_all(typed.as_bytes()).unwrap();
        assert_eq!(
            read_answer(&mut ours, soon(1000), &|| false, "8b7d"),
            Some(expected),
            "{typed:?}"
        );
    }
}

#[test]
fn an_answer_that_arrives_before_the_deadline_is_used() {
    let (mut ours, mut theirs) = UnixStream::pair().unwrap();
    let typist = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(40));
        theirs.write_all(b"approve 8b7d\n").unwrap();
        theirs
    });
    assert_eq!(
        read_answer(&mut ours, soon(2000), &|| false, "8b7d"),
        Some(ApprovalOutcome::AllowOnce)
    );
    typist.join().unwrap();
}

#[test]
fn a_late_answer_cannot_approve_the_next_prompt() {
    let (mut ours, mut theirs) = UnixStream::pair().unwrap();
    assert_eq!(read_answer(&mut ours, soon(30), &|| false, "8b7d"), None);

    theirs.write_all(b"approve 8b7d\napprove next\n").unwrap();

    assert_eq!(
        read_answer(&mut ours, soon(1000), &|| false, "next"),
        Some(ApprovalOutcome::Deny)
    );
    assert_eq!(
        read_answer(&mut ours, soon(1000), &|| false, "next"),
        Some(ApprovalOutcome::AllowOnce)
    );
}

#[test]
fn cancelled_readers_exit_without_waiting_for_input() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};

    for _ in 0..5 {
        let (mut ours, _theirs) = UnixStream::pair().unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let (finished, result) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            ready.send(()).unwrap();
            let answer = read_answer(&mut ours, soon(10_000), &|| flag.load(Ordering::Acquire), "8b7d");
            finished.send(answer).unwrap();
        });
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        cancelled.store(true, Ordering::Release);
        assert_eq!(result.recv_timeout(Duration::from_secs(2)).unwrap(), None);
        reader.join().unwrap();
    }
}

#[test]
fn a_terminal_discards_pending_input_and_rejects_an_old_prompt_answer() {
    use std::os::fd::FromRawFd;

    let (mut master_fd, mut slave_fd) = (-1, -1);
    let result = unsafe {
        libc::openpty(
            &mut master_fd,
            &mut slave_fd,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert_eq!(result, 0);
    let mut master = unsafe { std::fs::File::from_raw_fd(master_fd) };
    let mut slave = unsafe { std::fs::File::from_raw_fd(slave_fd) };
    master.write_all(b"approve old\n").unwrap();
    assert_eq!(poll_readable(slave.as_raw_fd(), 1000), Some(true));
    discard_pending_input(&slave);
    assert_eq!(poll_readable(slave.as_raw_fd(), 30), Some(false));
    master.write_all(b"approve old\n").unwrap();
    assert_eq!(
        read_answer(&mut slave, soon(1000), &|| false, "new"),
        Some(ApprovalOutcome::Deny)
    );
    master.write_all(b"approve new\n").unwrap();
    assert_eq!(
        read_answer(&mut slave, soon(1000), &|| false, "new"),
        Some(ApprovalOutcome::AllowOnce)
    );
}

#[test]
fn an_over_long_answer_is_denied_after_a_bounded_read() {
    let (mut ours, mut theirs) = UnixStream::pair().unwrap();
    theirs.write_all(&[b'y'; MAX_ANSWER_BYTES + 10]).unwrap();
    assert_eq!(
        read_answer(&mut ours, soon(1000), &|| false, "8b7d"),
        Some(ApprovalOutcome::Deny)
    );
    let mut rest = [0u8; 16];
    assert_eq!(std::io::Read::read(&mut ours, &mut rest).unwrap(), 10);
}

#[test]
fn a_closed_terminal_is_a_denial_not_a_hang() {
    let (mut ours, theirs) = UnixStream::pair().unwrap();
    drop(theirs);
    assert_eq!(
        read_answer(&mut ours, soon(1000), &|| false, "8b7d"),
        Some(ApprovalOutcome::Deny)
    );
}
