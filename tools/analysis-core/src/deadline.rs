//! Both batch and IDE requests use one cooperative timer for input reads and the kernel handle.

use std::{
    sync::mpsc::{self, RecvTimeoutError, Sender},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use crate::{RequestCancellation, cancellation::RequestStop};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeadlineError {
    TimerUnavailable,
    TimeoutOutOfRange,
}

pub struct Deadline {
    end: Instant,
    cancellation: RequestCancellation,
    stop: Sender<()>,
    timer: Option<JoinHandle<()>>,
}

impl Deadline {
    /// The timer owns no database handle and wakes immediately when a completed request drops it.
    pub(crate) fn start(
        timeout: Duration,
        cancellation: RequestCancellation,
    ) -> Result<Self, DeadlineError> {
        let end = Instant::now()
            .checked_add(timeout)
            .ok_or(DeadlineError::TimeoutOutOfRange)?;
        let (stop, receiver) = mpsc::channel();
        let request = cancellation.clone();
        let timer = std::thread::Builder::new()
            .name("eska-analysis-deadline".into())
            .spawn(move || {
                if receiver.recv_timeout(end.saturating_duration_since(Instant::now()))
                    == Err(RecvTimeoutError::Timeout)
                {
                    request.cancel_deadline();
                }
            })
            .map_err(|_| DeadlineError::TimerUnavailable)?;
        Ok(Self {
            end,
            cancellation,
            stop,
            timer: Some(timer),
        })
    }

    /// Recognize a zero/elapsed deadline immediately even if the timer thread has not run yet.
    pub(crate) fn stop_reason(&self) -> Option<RequestStop> {
        if Instant::now() >= self.end {
            self.cancellation.cancel_deadline();
        }
        self.cancellation.stop_reason()
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
