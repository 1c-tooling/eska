//! Keyboard-driven selection confined to the CLI presentation layer.

use std::io::{self, Write};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute, queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::localization::Localizer;

#[derive(Debug)]
pub(super) enum PromptError {
    Cancelled,
    Io,
}

pub(super) struct Selector {
    output: io::Stderr,
    active: bool,
    styled: bool,
}

impl Selector {
    pub(super) fn start() -> Result<Self, PromptError> {
        terminal::enable_raw_mode().map_err(|_| PromptError::Io)?;
        // Construct the guard before any terminal writes, so partial setup and
        // unwinding restore the screen, cursor and terminal input mode too.
        let mut selector = Self {
            output: io::stderr(),
            active: true,
            styled: std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty()),
        };
        execute!(
            selector.output,
            EnterAlternateScreen,
            Hide,
            EnableBracketedPaste
        )
        .map_err(|_| PromptError::Io)?;
        // Initialize the event source (including resize signal handling) before
        // the first frame; otherwise an immediate resize can be missed.
        event::poll(std::time::Duration::ZERO).map_err(|_| PromptError::Io)?;
        Ok(selector)
    }

    pub(super) fn choose(
        &mut self,
        localizer: &Localizer,
        title: &str,
        choices: &[(&str, &str)],
    ) -> Result<String, PromptError> {
        if choices.is_empty() || choices.len() > 9 {
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
            title,
            choices,
            selected,
            size,
            self.styled,
        )
        .and_then(|()| self.output.write_all(&frame))
        .and_then(|()| self.output.flush())
        .map_err(|_| PromptError::Io)
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

impl Drop for Selector {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Action {
    Move(usize),
    Confirm(usize),
    Cancel,
    Ignore,
}

fn action(key: KeyEvent, selected: usize, count: usize) -> Action {
    if key.kind == KeyEventKind::Release {
        return Action::Ignore;
    }
    if key.code == KeyCode::Esc
        || (key.modifiers == KeyModifiers::CONTROL && matches!(key.code, KeyCode::Char('c' | 'd')))
    {
        return Action::Cancel;
    }
    if count == 0
        || key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return Action::Ignore;
    }
    match navigation_key(key.code) {
        KeyCode::Up | KeyCode::Char('k') => Action::Move((selected + count - 1) % count),
        KeyCode::Down | KeyCode::Char('j') => Action::Move((selected + 1) % count),
        KeyCode::Home | KeyCode::Char('g') => Action::Move(0),
        KeyCode::End | KeyCode::Char('G') => Action::Move(count - 1),
        // Holding Enter/a digit must not also confirm the next step on platforms
        // that distinguish repeated key events (e.g. Windows).
        KeyCode::Enter if key.kind == KeyEventKind::Press => Action::Confirm(selected),
        KeyCode::Char(digit @ '1'..='9') if key.kind == KeyEventKind::Press => {
            let index = digit as usize - '1' as usize;
            if index < count {
                Action::Confirm(index)
            } else {
                Action::Ignore
            }
        }
        _ => Action::Ignore,
    }
}

// Legacy terminal input contains characters, not physical key positions.
// Keep the standard Russian-layout fallback local to menu navigation; this is
// neither general transliteration nor locale-dependent input normalization.
const fn navigation_key(code: KeyCode) -> KeyCode {
    match code {
        KeyCode::Char('о') => KeyCode::Char('j'),
        KeyCode::Char('л') => KeyCode::Char('k'),
        KeyCode::Char('п') => KeyCode::Char('g'),
        KeyCode::Char('П') => KeyCode::Char('G'),
        _ => code,
    }
}

fn fits((width, height): (u16, u16), count: usize) -> bool {
    width >= 50 && usize::from(height) >= count + 10
}

// Menu text comes only from bundled RU/EN resources: all glyphs occupy one
// terminal cell. Do not pass arbitrary paths or user strings to this renderer.
fn render(
    output: &mut impl Write,
    localizer: &Localizer,
    title: &str,
    choices: &[(&str, &str)],
    selected: usize,
    size: (u16, u16),
    styled: bool,
) -> io::Result<()> {
    queue!(
        output,
        ResetColor,
        SetAttribute(Attribute::Reset),
        MoveTo(0, 0),
        Clear(ClearType::All)
    )?;
    let (width, height) = size;
    if !fits(size, choices.len()) {
        line(output, 0, &localizer.text("select-resize"), width, height)?;
        return line(
            output,
            2,
            &localizer.text("select-cancel-hint"),
            width,
            height,
        );
    }
    if styled {
        queue!(output, SetAttribute(Attribute::Bold))?;
    }
    line(output, 1, &localizer.text("new-tui-title"), width, height)?;
    queue!(output, SetAttribute(Attribute::Reset))?;
    line(output, 3, &localizer.text(title), width, height)?;
    for (index, (value, label)) in choices.iter().enumerate() {
        if styled && index == selected {
            queue!(
                output,
                SetForegroundColor(Color::Cyan),
                SetAttribute(Attribute::Bold)
            )?;
        }
        let marker = if index == selected { "›" } else { " " };
        let text = format!(
            "{marker} {}  {} ({value})",
            index + 1,
            localizer.text(label)
        );
        let row = u16::try_from(index).map_err(io::Error::other)? + 5;
        line(output, row, &text, width, height)?;
        queue!(output, ResetColor, SetAttribute(Attribute::Reset))?;
    }
    if styled {
        queue!(output, SetAttribute(Attribute::Dim))?;
    }
    let footer = u16::try_from(choices.len()).map_err(io::Error::other)? + 6;
    for (row, key) in [
        (footer, "select-navigation-hint"),
        (footer + 1, "select-digit-hint"),
        (footer + 2, "select-cancel-hint"),
    ] {
        line(output, row, &localizer.text(key), width, height)?;
    }
    queue!(output, ResetColor, SetAttribute(Attribute::Reset))
}

fn line(output: &mut impl Write, row: u16, text: &str, width: u16, height: u16) -> io::Result<()> {
    if row < height && width > 3 {
        let text: String = text.chars().take(usize::from(width - 3)).collect();
        queue!(output, MoveTo(2, row), Print(text))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::localization::Locale;

    fn press(code: KeyCode, selected: usize) -> Action {
        action(KeyEvent::new(code, KeyModifiers::NONE), selected, 4)
    }

    #[test]
    fn arrows_and_vim_keys_move_and_wrap_without_confirming() {
        for code in [KeyCode::Down, KeyCode::Char('j')] {
            assert_eq!(press(code, 0), Action::Move(1));
            assert_eq!(press(code, 3), Action::Move(0));
        }
        for code in [KeyCode::Up, KeyCode::Char('k')] {
            assert_eq!(press(code, 2), Action::Move(1));
            assert_eq!(press(code, 0), Action::Move(3));
        }
        for code in [KeyCode::Home, KeyCode::Char('g')] {
            assert_eq!(press(code, 3), Action::Move(0));
        }
        for code in [KeyCode::End, KeyCode::Char('G')] {
            assert_eq!(press(code, 0), Action::Move(3));
        }
        assert_eq!(press(KeyCode::Enter, 2), Action::Confirm(2));
    }

    #[test]
    fn digits_select_immediately_and_ignore_out_of_range_values() {
        for (index, digit) in ('1'..='4').enumerate() {
            assert_eq!(press(KeyCode::Char(digit), 0), Action::Confirm(index));
        }
        for digit in ['0', '5', '9', 'x', ' '] {
            assert_eq!(press(KeyCode::Char(digit), 0), Action::Ignore);
        }
    }

    #[test]
    fn russian_navigation_matches_latin_events_including_modifiers_and_repeats() {
        for (latin, russian) in [('j', 'о'), ('k', 'л'), ('g', 'п'), ('G', 'П')] {
            for selected in 0..4 {
                for modifiers in [
                    KeyModifiers::NONE,
                    KeyModifiers::SHIFT,
                    KeyModifiers::CONTROL,
                    KeyModifiers::ALT,
                    KeyModifiers::SUPER,
                ] {
                    for kind in [
                        KeyEventKind::Press,
                        KeyEventKind::Repeat,
                        KeyEventKind::Release,
                    ] {
                        let latin = KeyEvent::new_with_kind(KeyCode::Char(latin), modifiers, kind);
                        let russian =
                            KeyEvent::new_with_kind(KeyCode::Char(russian), modifiers, kind);
                        assert_eq!(action(russian, selected, 4), action(latin, selected, 4));
                    }
                }
            }
        }
    }

    #[test]
    fn fallback_does_not_translate_unrelated_keys_or_control_shortcuts() {
        for character in ['р', 'д', 'с', 'в', 'О', 'Л', 'J', 'K', 'é', 'λ'] {
            assert_eq!(
                navigation_key(KeyCode::Char(character)),
                KeyCode::Char(character)
            );
            assert_eq!(press(KeyCode::Char(character), 0), Action::Ignore);
        }
        for character in ['о', 'л', 'п', 'П', 'с', 'в'] {
            assert_eq!(
                action(
                    KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL),
                    0,
                    4
                ),
                Action::Ignore
            );
        }
    }

    #[test]
    fn cancellation_and_modifiers_are_explicit() {
        assert_eq!(press(KeyCode::Esc, 0), Action::Cancel);
        for code in ['c', 'd'] {
            assert_eq!(
                action(
                    KeyEvent::new(KeyCode::Char(code), KeyModifiers::CONTROL),
                    0,
                    4
                ),
                Action::Cancel
            );
            assert_eq!(press(KeyCode::Char(code), 0), Action::Ignore);
        }
        for modifier in [
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::SUPER,
        ] {
            for code in [
                KeyCode::Enter,
                KeyCode::Down,
                KeyCode::Char('1'),
                KeyCode::Char('j'),
            ] {
                assert_eq!(action(KeyEvent::new(code, modifier), 0, 4), Action::Ignore);
            }
        }
    }

    #[test]
    fn release_and_repeat_cannot_confirm_the_next_menu() {
        for code in [
            KeyCode::Enter,
            KeyCode::Char('1'),
            KeyCode::Down,
            KeyCode::Esc,
        ] {
            let mut key = KeyEvent::new(code, KeyModifiers::NONE);
            key.kind = KeyEventKind::Release;
            assert_eq!(action(key, 0, 4), Action::Ignore);
        }
        for code in [KeyCode::Enter, KeyCode::Char('1')] {
            let mut key = KeyEvent::new(code, KeyModifiers::NONE);
            key.kind = KeyEventKind::Repeat;
            assert_eq!(action(key, 0, 4), Action::Ignore);
        }
        let mut key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        key.kind = KeyEventKind::Repeat;
        assert_eq!(action(key, 0, 4), Action::Move(1));
    }

    const CHOICES: [(&str, &str); 4] = [
        ("configuration", "new-type-configuration"),
        ("extension", "new-type-extension"),
        ("processing", "new-type-processing"),
        ("report", "new-type-report"),
    ];

    #[test]
    fn menus_are_localized_and_selection_is_visible_without_color() {
        for locale in [Locale::RuRu, Locale::EnUs] {
            let localizer = Localizer::try_new(locale).expect("locale");
            for styled in [true, false] {
                let mut output = Vec::new();
                render(
                    &mut output,
                    &localizer,
                    "new-type-menu",
                    &CHOICES,
                    1,
                    (80, 24),
                    styled,
                )
                .expect("render");
                let text = String::from_utf8(output).expect("UTF-8");
                for (_, label) in CHOICES {
                    assert!(text.contains(&localizer.text(label)), "{text}");
                }
                assert!(text.contains(&format!(
                    "› 2  {} (extension)",
                    localizer.text("new-type-extension")
                )));
                assert_eq!(text.matches('›').count(), 1);
                assert!(text.contains(&localizer.text("select-navigation-hint")));
                assert!(text.contains(&localizer.text("select-digit-hint")));
                assert_eq!(text.contains("\x1b[1m"), styled);
            }
        }
    }

    #[test]
    fn small_terminal_requests_resize_without_showing_invisible_choices() {
        let localizer = Localizer::try_new(Locale::EnUs).expect("locale");
        for size in [(49, 24), (80, 13), (0, 0), (2, 2)] {
            assert!(!fits(size, 4));
            let mut output = Vec::new();
            render(
                &mut output,
                &localizer,
                "new-type-menu",
                &CHOICES,
                0,
                size,
                true,
            )
            .expect("render small terminal");
            let text = String::from_utf8(output).expect("UTF-8");
            assert!(!text.contains("configuration"));
            if size.0 > 40 {
                assert!(text.contains(&localizer.text("select-resize")));
            }
        }
        assert!(fits((50, 14), 4));
        let mut output = Vec::new();
        line(&mut output, 0, "АБВГД", 6, 1).expect("clipped line");
        assert_eq!(String::from_utf8(output).expect("UTF-8"), "\x1b[1;3HАБВ");
    }

    #[test]
    fn rendering_propagates_output_errors() {
        struct BrokenOutput;
        impl Write for BrokenOutput {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("broken output"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let localizer = Localizer::try_new(Locale::EnUs).expect("locale");
        assert!(
            render(
                &mut BrokenOutput,
                &localizer,
                "new-type-menu",
                &CHOICES,
                0,
                (80, 24),
                true
            )
            .is_err()
        );
    }
}
