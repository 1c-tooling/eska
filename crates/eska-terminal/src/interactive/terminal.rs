//! Owns the raw-mode/alternate-screen session and restores it on drop.

use std::io::{self, Write};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, DisableBracketedPaste, EnableBracketedPaste},
    execute,
    style::{Attribute, ResetColor, SetAttribute},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};

pub(super) struct Terminal {
    output: io::Stderr,
    active: bool,
    styled: bool,
}

impl Terminal {
    pub(super) fn start() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        // Construct the guard before any terminal writes, so partial setup and
        // unwinding restore the screen, cursor and terminal input mode too.
        let mut terminal = Self {
            output: io::stderr(),
            active: true,
            styled: std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty()),
        };
        execute!(
            terminal.output,
            EnterAlternateScreen,
            Hide,
            EnableBracketedPaste
        )?;
        // Initialize the event source (including resize signal handling) before
        // the first frame; otherwise an immediate resize can be missed.
        event::poll(std::time::Duration::ZERO)?;
        Ok(terminal)
    }

    pub(super) const fn styled(&self) -> bool {
        self.styled
    }

    pub(super) fn write_frame(&mut self, frame: &[u8]) -> io::Result<()> {
        self.output.write_all(frame)?;
        self.output.flush()
    }

    pub(super) fn finish(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        let screen = execute!(
            self.output,
            ResetColor,
            SetAttribute(Attribute::Reset),
            Show,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        // Always attempt input restoration even if writing to stderr failed.
        let input = terminal::disable_raw_mode();
        if screen.is_ok() && input.is_ok() {
            self.active = false;
        }
        screen.and(input)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
