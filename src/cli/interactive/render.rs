//! Menu layout, styling, clipping and minimum terminal size.

use std::io::{self, Write};

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{Clear, ClearType},
};

use crate::cli::localization::Localizer;

pub(super) fn fits((width, height): (u16, u16), count: usize) -> bool {
    width >= 50 && usize::from(height) >= count + 10
}

// Menu text comes only from bundled RU/EN resources: all glyphs occupy one
// terminal cell. Do not pass arbitrary paths or user strings to this renderer.
pub(super) fn render(
    output: &mut impl Write,
    localizer: &Localizer,
    (heading, title): (&str, &str),
    choices: &[(&str, &str)],
    selected: usize,
    size: (u16, u16),
    styled: bool,
) -> io::Result<()> {
    let choices: Vec<_> = choices
        .iter()
        .map(|(value, label)| ((*value).to_owned(), localizer.text(label)))
        .collect();
    render_values(
        output,
        localizer,
        (heading, title),
        &choices,
        selected,
        size,
        styled,
    )
}

pub(super) fn render_values(
    output: &mut impl Write,
    localizer: &Localizer,
    (heading, title): (&str, &str),
    choices: &[(String, String)],
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
    line(output, 1, &localizer.text(heading), width, height)?;
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
        let text = format!("{marker} {}  {} ({value})", index + 1, label);
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
    use crate::cli::{interactive::PROJECT_TYPE_CHOICES as CHOICES, localization::Locale};

    #[test]
    fn menus_are_localized_and_selection_is_visible_without_color() {
        for locale in [Locale::RuRu, Locale::EnUs] {
            let localizer = Localizer::try_new(locale).expect("locale");
            for styled in [true, false] {
                let mut output = Vec::new();
                render(
                    &mut output,
                    &localizer,
                    ("new-tui-title", "new-type-menu"),
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
                ("new-tui-title", "new-type-menu"),
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
                ("new-tui-title", "new-type-menu"),
                &CHOICES,
                0,
                (80, 24),
                true
            )
            .is_err()
        );
    }
}
