//! Key bindings and Russian-layout navigation fallback, independent of rendering.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Debug, Eq, PartialEq)]
pub(super) enum Action {
    Move(usize),
    Confirm(usize),
    Cancel,
    Ignore,
}

pub(super) fn action(key: KeyEvent, selected: usize, count: usize) -> Action {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
