//! First-cause latching and timer lifetime do not depend on transport scheduling.

use super::*;

/// Client cancellation and deadline expiry cannot overwrite whichever cause was recorded first.
#[test]
fn first_stop_reason_survives_zero_deadline_and_late_client_cancellation() {
    let client = RequestCancellation::default();
    client.cancel();
    let deadline = Deadline::start(Duration::ZERO, client.clone()).unwrap();
    assert_eq!(deadline.stop_reason(), Some(RequestStop::Cancelled));
    assert!(client.input.is_cancelled() && client.query.is_cancelled());
    let timed = RequestCancellation::default();
    let deadline = Deadline::start(Duration::ZERO, timed.clone()).unwrap();
    assert_eq!(deadline.stop_reason(), Some(RequestStop::DeadlineExceeded));
    timed.cancel();
    assert_eq!(timed.stop_reason(), Some(RequestStop::DeadlineExceeded));
    assert!(timed.input.is_cancelled() && timed.query.is_cancelled());
}

/// An impossible duration fails before spawning a timer or poisoning any source/kernel token.
#[test]
fn duration_overflow_is_structured_and_does_not_cancel_the_request() {
    let control = RequestCancellation::default();
    assert_eq!(
        Deadline::start(Duration::MAX, control.clone()).err(),
        Some(DeadlineError::TimeoutOutOfRange)
    );
    assert!(!control.is_cancelled());
    assert!(!control.input.is_cancelled());
    assert!(!control.query.is_cancelled());
}

/// Dropping a finished request wakes its long timer rather than waiting for its original expiry.
#[test]
fn completed_timer_stops_without_cancelling_its_request() {
    let control = RequestCancellation::default();
    let deadline = Deadline::start(Duration::from_secs(300), control.clone()).unwrap();
    let (done, receiver) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        drop(deadline);
        done.send(()).unwrap();
    });
    receiver.recv_timeout(Duration::from_secs(10)).unwrap();
    worker.join().unwrap();
    assert!(!control.is_cancelled());
    assert!(!control.input.is_cancelled());
    assert!(!control.query.is_cancelled());
}
