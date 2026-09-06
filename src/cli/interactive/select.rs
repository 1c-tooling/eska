//! Selection event loop; delegates input, rendering and terminal lifecycle.

use std::io;

use crossterm::{
    event::{self, Event},
    terminal,
};

use crate::cli::localization::Localizer;

use super::{
    PromptError,
    keyboard::{Action, action},
    render::{fits, render, render_values},
    terminal::Terminal,
};

pub(in crate::cli) struct Selector {
    heading: &'static str,
    terminal: Terminal,
}

impl Selector {
    pub(in crate::cli) fn start(heading: &'static str) -> Result<Self, PromptError> {
        let terminal = Terminal::start().map_err(|_| PromptError::Io)?;
        Ok(Self { heading, terminal })
    }

    pub(in crate::cli) fn choose(
        &mut self,
        localizer: &Localizer,
        title: &str,
        choices: &[(&str, &str)],
    ) -> Result<String, PromptError> {
        if choices.is_empty() {
            return Err(PromptError::Io);
        }
        let mut selected = 0;
        let mut size = terminal::size().map_err(|_| PromptError::Io)?;
        self.draw(localizer, title, choices, selected, size)?;
        loop {
            match event::read().map_err(|_| PromptError::Io)? {
                Event::Resize(width, height) => {
                    size = (width, height);
                    self.draw(localizer, title, choices, selected, size)?;
                }
                Event::Key(key) => {
                    let action = action(key, selected, choices.len());
                    if action == Action::Cancel {
                        return Err(PromptError::Cancelled);
                    }
                    // Never confirm an option the user cannot currently see.
                    if !fits(size, choices.len()) {
                        continue;
                    }
                    match action {
                        Action::Move(index) => {
                            selected = index;
                            self.draw(localizer, title, choices, selected, size)?;
                        }
                        Action::Confirm(index) => return Ok(choices[index].0.to_owned()),
                        Action::Ignore | Action::Cancel => {}
                    }
                }
                // In particular, pasted digits must not confirm two menus and
                // create a project accidentally. Only key presses select items.
                _ => {}
            }
        }
    }

    pub(in crate::cli) fn choose_values(
        &mut self,
        localizer: &Localizer,
        title: &str,
        choices: &[(String, String)],
    ) -> Result<String, PromptError> {
        if choices.is_empty() {
            return Err(PromptError::Io);
        }
        let mut selected = 0;
        let mut size = terminal::size().map_err(|_| PromptError::Io)?;
        self.draw_values(localizer, title, choices, selected, size)?;
        loop {
            match event::read().map_err(|_| PromptError::Io)? {
                Event::Resize(width, height) => {
                    size = (width, height);
                    self.draw_values(localizer, title, choices, selected, size)?;
                }
                Event::Key(key) => {
                    let action = action(key, selected, choices.len());
                    if action == Action::Cancel {
                        return Err(PromptError::Cancelled);
                    }
                    if !fits(size, choices.len()) {
                        continue;
                    }
                    match action {
                        Action::Move(index) => {
                            selected = index;
                            self.draw_values(localizer, title, choices, selected, size)?;
                        }
                        Action::Confirm(index) => return Ok(choices[index].0.clone()),
                        Action::Ignore | Action::Cancel => {}
                    }
                }
                _ => {}
            }
        }
    }

    fn draw(
        &mut self,
        localizer: &Localizer,
        title: &str,
        choices: &[(&str, &str)],
        selected: usize,
        size: (u16, u16),
    ) -> Result<(), PromptError> {
        let mut frame = Vec::new();
        render(
            &mut frame,
            localizer,
            (self.heading, title),
            choices,
            selected,
            size,
            self.terminal.styled(),
        )
        .and_then(|()| self.terminal.write_frame(&frame))
        .map_err(|_| PromptError::Io)
    }

    fn draw_values(
        &mut self,
        localizer: &Localizer,
        title: &str,
        choices: &[(String, String)],
        selected: usize,
        size: (u16, u16),
    ) -> Result<(), PromptError> {
        let mut frame = Vec::new();
        render_values(
            &mut frame,
            localizer,
            (self.heading, title),
            choices,
            selected,
            size,
            self.terminal.styled(),
        )
        .and_then(|()| self.terminal.write_frame(&frame))
        .map_err(|_| PromptError::Io)
    }

    pub(in crate::cli) fn finish(&mut self) -> io::Result<()> {
        self.terminal.finish()
    }
}
