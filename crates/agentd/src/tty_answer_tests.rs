use super::*;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

fn never() -> AtomicBool {
    AtomicBool::new(false)
}

fn soon(milliseconds: u64) -> Instant {
    Instant::now() + Duration::from_millis(milliseconds)
}

#[test]
fn silence_until_the_deadline_is_a_timeout_that_returns_promptly() {
    let (mut ours, _theirs) = UnixStream::pair().unwrap();
    let started = Instant::now();
    assert_eq!(read_answer(&mut ours, soon(80), &never()), None);
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(70) && elapsed < Duration::from_secs(2),
        "{elapsed:?}"
    );
}

#[test]
fn only_a_lone_y_allows_and_everything_else_denies() {
    for (typed, expected) in [
        ("y\n", ApprovalOutcome::AllowOnce),
        ("Y\n", ApprovalOutcome::AllowOnce),
        ("n\n", ApprovalOutcome::Deny),
        ("\n", ApprovalOutcome::Deny),
        ("yes\n", ApprovalOutcome::Deny),
    ] {
        let (mut ours, mut theirs) = UnixStream::pair().unwrap();
        theirs.write_all(typed.as_bytes()).unwrap();
        assert_eq!(
            read_answer(&mut ours, soon(1000), &never()),
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
        theirs.write_all(b"y\n").unwrap();
        theirs
    });
    assert_eq!(
        read_answer(&mut ours, soon(2000), &never()),
        Some(ApprovalOutcome::AllowOnce)
    );
    typist.join().unwrap();
}

#[test]
fn a_timed_out_read_leaves_a_later_answer_unread_instead_of_swallowing_it() {
    let (mut ours, mut theirs) = UnixStream::pair().unwrap();
    assert_eq!(read_answer(&mut ours, soon(30), &never()), None);

    theirs.write_all(b"y\nrest\n").unwrap();

    assert_eq!(
        read_answer(&mut ours, soon(1000), &never()),
        Some(ApprovalOutcome::AllowOnce)
    );
    assert_eq!(
        read_answer(&mut ours, soon(1000), &never()),
        Some(ApprovalOutcome::Deny),
        "the second line is read by the next call, not lost"
    );
}

#[test]
fn an_over_long_answer_is_denied_after_a_bounded_read() {
    let (mut ours, mut theirs) = UnixStream::pair().unwrap();
    theirs.write_all(&[b'y'; MAX_ANSWER_BYTES + 10]).unwrap();
    assert_eq!(
        read_answer(&mut ours, soon(1000), &never()),
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
        read_answer(&mut ours, soon(1000), &never()),
        Some(ApprovalOutcome::Deny)
    );
}

#[test]
fn a_stop_request_ends_a_waiting_read_within_one_poll_slice() {
    let (mut ours, _theirs) = UnixStream::pair().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let setter = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            stop.store(true, Ordering::Release);
        })
    };
    let started = Instant::now();
    assert_eq!(read_answer(&mut ours, soon(30_000), &stop), None);
    assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    setter.join().unwrap();
}

#[test]
fn a_stopped_reader_leaves_the_next_prompts_answer_unread() {
    let (mut ours, mut theirs) = UnixStream::pair().unwrap();
    let stop = AtomicBool::new(true);
    assert_eq!(read_answer(&mut ours, soon(30_000), &stop), None);
    theirs.write_all(b"y\n").unwrap();
    assert_eq!(
        read_answer(&mut ours, soon(1000), &never()),
        Some(ApprovalOutcome::AllowOnce)
    );
}
