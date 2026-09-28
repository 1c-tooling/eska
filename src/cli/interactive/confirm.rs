//! Inline confirmation keeps the full replacement preview visible in terminal scrollback.

use std::{io, time::Duration};

use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute, terminal,
};

use super::PromptError;

/// Accept only an explicit overwrite key, ignoring pasted text and repeated key events.
pub(in crate::cli) fn confirm_overwrite(
    interrupted: impl Fn() -> bool,
) -> Result<bool, PromptError> {
    terminal::enable_raw_mode().map_err(|_| PromptError::Io)?;
    let _guard = ConfirmationTerminal;
    execute!(io::stderr(), EnableBracketedPaste).map_err(|_| PromptError::Io)?;
    loop {
        if interrupted() {
            return Ok(false);
        }
        if !event::poll(Duration::from_millis(100)).map_err(|_| PromptError::Io)? {
            continue;
        }
        if let Event::Key(key) = event::read().map_err(|_| PromptError::Io)? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('c' | 'd'))
            {
                return Ok(false);
            }
            if key.modifiers.is_empty() {
                match key.code {
                    KeyCode::Char('2') => return Ok(true),
                    KeyCode::Char('1') | KeyCode::Enter | KeyCode::Esc => return Ok(false),
                    _ => {}
                }
            }
        }
    }
}

struct ConfirmationTerminal;

impl Drop for ConfirmationTerminal {
    /// Restore terminal input even after a cancelled prompt or failed event read.
    fn drop(&mut self) {
        let _ = execute!(io::stderr(), DisableBracketedPaste);
        let _ = terminal::disable_raw_mode();
    }
}
