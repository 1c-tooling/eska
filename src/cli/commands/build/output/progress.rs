//! Lifecycle and synchronized stderr access for the build spinner.

use std::{
    io::{self, Write as _},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Sender},
    },
    thread,
    time::Duration,
};

use crossterm::{
    cursor::MoveToColumn,
    queue,
    terminal::{Clear, ClearType},
};

use super::decorate_status;

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

struct ProgressState {
    output: Mutex<()>,
    frame: AtomicUsize,
    message: String,
    styled: bool,
}

pub(in crate::cli::commands::build) struct ProgressLine {
    state: Arc<ProgressState>,
    stop: Sender<()>,
    worker: Option<thread::JoinHandle<io::Result<()>>>,
    active: bool,
}

impl ProgressLine {
    /// Start a terminal-owned spinner that remains below streamed diagnostics.
    pub(in crate::cli::commands::build) fn start(message: String, styled: bool) -> Self {
        let state = Arc::new(ProgressState {
            output: Mutex::new(()),
            frame: AtomicUsize::new(0),
            message,
            styled,
        });
        let (stop, receiver) = mpsc::channel();
        let worker_state = Arc::clone(&state);
        let worker = thread::spawn(move || {
            loop {
                draw_progress(&worker_state)?;
                match receiver.recv_timeout(Duration::from_millis(80)) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        worker_state.frame.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
        Self {
            state,
            stop,
            worker: Some(worker),
            active: true,
        }
    }

    /// Clear the spinner, write one complete diagnostic line, then restore it.
    pub(super) fn write_diagnostic(&self, line: &[u8]) -> io::Result<()> {
        let _guard = lock_progress_output(&self.state)?;
        let mut stderr = io::stderr().lock();
        clear_progress(&mut stderr)?;
        stderr.write_all(line)?;
        if !line.ends_with(b"\n") {
            stderr.write_all(b"\n")?;
        }
        draw_progress_locked(&mut stderr, &self.state)?;
        stderr.flush()
    }

    /// Stop animation and remove its final line before result presentation.
    pub(in crate::cli::commands::build) fn finish(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let _ = self.stop.send(());
        let worker_result = self
            .worker
            .take()
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| io::Error::other("build progress thread panicked"))?
            })
            .transpose();
        let clear_result = {
            let _guard = lock_progress_output(&self.state)?;
            let mut stderr = io::stderr().lock();
            clear_progress(&mut stderr)?;
            stderr.flush()
        };
        worker_result.and(clear_result)
    }
}

impl Drop for ProgressLine {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

/// Lock the multi-write terminal sequence shared with the animation thread.
fn lock_progress_output(state: &ProgressState) -> io::Result<std::sync::MutexGuard<'_, ()>> {
    state
        .output
        .lock()
        .map_err(|_| io::Error::other("build progress output lock was poisoned"))
}

/// Draw the current animation frame while acquiring the shared terminal lock.
fn draw_progress(state: &ProgressState) -> io::Result<()> {
    let _guard = lock_progress_output(state)?;
    let mut stderr = io::stderr().lock();
    draw_progress_locked(&mut stderr, state)?;
    stderr.flush()
}

/// Draw the current animation frame as the terminal's last line.
fn draw_progress_locked(stderr: &mut impl io::Write, state: &ProgressState) -> io::Result<()> {
    let frame = state.frame.load(Ordering::Relaxed) % SPINNER_FRAMES.len();
    let message = decorate_status(SPINNER_FRAMES[frame], &state.message, state.styled, "36");
    queue!(
        stderr,
        MoveToColumn(0),
        Clear(ClearType::CurrentLine),
        crossterm::style::Print(message)
    )
}

/// Remove the transient progress line without affecting completed diagnostics.
fn clear_progress(stderr: &mut impl io::Write) -> io::Result<()> {
    queue!(stderr, MoveToColumn(0), Clear(ClearType::CurrentLine))
}
