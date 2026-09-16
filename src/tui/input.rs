use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthStr;
use zeroize::Zeroize;

/// Byte cursor is always on a UTF-8 boundary.
#[derive(Default)]
pub(super) struct Input {
    pub text: String,
    pub cursor: usize,
}

impl From<&str> for Input {
    fn from(text: &str) -> Self {
        Self {
            text: text.into(),
            cursor: text.len(),
        }
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        self.text.zeroize();
    }
}

impl Input {
    pub fn insert(&mut self, text: &str, multiline: bool) {
        let clean: String = text
            .chars()
            .filter(|c| !c.is_control() || (multiline && *c == '\n'))
            .collect();
        self.text.insert_str(self.cursor, &clean);
        self.cursor += clean.len();
    }

    pub fn key(&mut self, key: KeyEvent, multiline: bool) {
        match key.code {
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert(&c.to_string(), multiline)
            }
            KeyCode::Left => {
                self.cursor = self.text[..self.cursor]
                    .char_indices()
                    .last()
                    .map_or(0, |(i, _)| i)
            }
            KeyCode::Right => {
                if let Some(c) = self.text[self.cursor..].chars().next() {
                    self.cursor += c.len_utf8();
                }
            }
            KeyCode::Home => {
                self.cursor = self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1)
            }
            KeyCode::End => {
                self.cursor += self.text[self.cursor..]
                    .find('\n')
                    .unwrap_or(self.text.len() - self.cursor)
            }
            KeyCode::Backspace if self.cursor > 0 => {
                let previous = self.text[..self.cursor].char_indices().last().unwrap().0;
                self.text.drain(previous..self.cursor);
                self.cursor = previous;
            }
            KeyCode::Delete if self.cursor < self.text.len() => {
                self.text.remove(self.cursor);
            }
            KeyCode::Enter if multiline => self.insert("\n", true),
            _ => {}
        }
    }

    pub fn position(&self) -> (usize, usize) {
        let prefix = &self.text[..self.cursor];
        (
            prefix.chars().filter(|c| *c == '\n').count(),
            prefix.rsplit('\n').next().unwrap_or("").width(),
        )
    }

    pub fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_unicode_keeps_byte_cursor_valid() {
        let mut input = Input::from("a界é");
        input.key(KeyCode::Left.into(), false);
        input.key(KeyCode::Backspace.into(), false);
        input.insert("🙂", false);
        assert_eq!(input.text, "a🙂é");
        assert_eq!(input.position(), (0, 3));
        input.key(KeyCode::Delete.into(), false);
        assert_eq!(input.take(), "a🙂");
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn paste_retains_only_allowed_controls() {
        let mut input = Input::default();
        input.insert("one\r\ntwo\u{1b}", true);
        assert_eq!(input.text, "one\ntwo");
    }
}
