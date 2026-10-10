//! One timer per batch forwards the deadline to bounded input reads and the active kernel handle.

use std::{
    sync::mpsc::{self, RecvTimeoutError, Sender},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use super::{BatchError, StopReason};
use crate::RequestCancellation;

pub struct Deadline {
    end: Instant,
    cancellation: RequestCancellation,
    stop: Sender<()>,
    timer: Option<JoinHandle<()>>,
}

impl Deadline {
    /// The timer owns no database handle and wakes immediately when a completed batch drops it.
    pub fn start(timeout: Duration, cancellation: RequestCancellation) -> Result<Self, BatchError> {
        let end = Instant::now()
            .checked_add(timeout)
            .ok_or(BatchError::TimeoutOutOfRange)?;
        let (stop, receiver) = mpsc::channel();
        let request = cancellation.clone();
        let timer = std::thread::Builder::new()
            .name("eska-check-deadline".into())
            .spawn(move || {
                if receiver.recv_timeout(end.saturating_duration_since(Instant::now()))
                    == Err(RecvTimeoutError::Timeout)
                {
                    request.cancel();
                }
            })
            .map_err(|_| BatchError::TimerUnavailable)?;
        Ok(Self {
            end,
            cancellation,
            stop,
            timer: Some(timer),
        })
    }

    /// Recognize a zero/elapsed deadline immediately even if the timer thread has not run yet.
    pub fn stop_reason(&self) -> Option<StopReason> {
        if Instant::now() >= self.end {
            self.cancellation.cancel();
            Some(StopReason::DeadlineExceeded)
        } else if self.cancellation.input.is_cancelled() {
            Some(StopReason::Cancelled)
        } else {
            None
        }
    }
}

impl Drop for Deadline {
    /// Cancellation of the timer itself cannot delay exit until the originally configured timeout.
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(timer) = self.timer.take() {
            let _ = timer.join();
        }
    }
}
