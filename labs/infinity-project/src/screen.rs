//! How far down the screen the chat has drawn.
//!
//! The header is drawn on the first row of a screen that starts empty, and
//! stays there until the lines under it have filled the screen: the next one
//! scrolls it off, for good. Until then its title can be drawn again where
//! it is, which is how the title follows the model (see `Ui::retitle`).
//!
//! [`Screen`] tells which of the two it is. It follows the cursor down from
//! the first row: it reads what the chat writes to the terminal (see
//! [`Tracked`]), and is told what the line editor draws. Where it cannot
//! know, it says that the header is gone: a title that is not drawn again is
//! only out of date, and one drawn over a row of the conversation is not.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use richrs::console::ConsoleWriter;
use richrs::prelude::Result;
use unicode_width::UnicodeWidthChar;

/// Rows left out of the count: what the line editor draws while a line is
/// typed, and then erases, is only known roughly.
const MARGIN: usize = 2;
/// The columns between the stops of a tab.
const TAB_STOP: usize = 8;

/// The cursor's way down a screen, from its first row.
#[derive(Debug, Default)]
pub struct Screen {
    /// The columns and rows of the screen when it started empty. `None`
    /// when what is on the screen is not known.
    size: Option<(usize, usize)>,
    row: usize,
    col: usize,
    /// The lowest row drawn on, as a row of a screen that never scrolls.
    lowest: usize,
    /// Where an escape sequence saved the cursor.
    saved: Option<(usize, usize)>,
    /// The escape sequence being read.
    escape: Escape,
}

/// What has been read of an escape sequence.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Escape {
    #[default]
    None,
    /// The escape character.
    Start,
    /// A control sequence, `ESC [`, up to the character that ends it.
    /// `bare` as long as it has no parameters.
    Control { bare: bool },
    /// An operating system command, `ESC ]`, up to a bell or an escape.
    System,
}

impl Screen {
    /// The screen is empty, `cols` by `rows`, with the cursor on its first
    /// row.
    pub fn start(&mut self, cols: usize, rows: usize) {
        *self = Self {
            size: Some((cols, rows)),
            ..Self::default()
        };
    }

    /// What is on the screen is no longer known.
    pub fn lose(&mut self) {
        self.size = None;
    }

    /// Whether nothing has been drawn since the screen started empty.
    pub fn is_empty(&self) -> bool {
        self.size.is_some() && (self.lowest, self.col) == (0, 0)
    }

    /// Whether the first row of the screen, now `cols` by `rows`, is still
    /// the first that was drawn on: nothing has scrolled it off.
    pub fn holds_first_row(&self, cols: usize, rows: usize) -> bool {
        // On a screen of another size the terminal has laid the lines out
        // again, its own way.
        self.size == Some((cols, rows)) && self.lowest + MARGIN < rows
    }

    /// `rows` under the cursor were drawn on and erased, or may have been.
    pub fn reach(&mut self, rows: usize) {
        self.lowest = self.lowest.max(self.row + rows);
    }

    /// Follows `text` as the terminal draws it.
    pub fn write(&mut self, text: &str) {
        let Some((cols, _)) = self.size else {
            return;
        };
        for c in text.chars() {
            self.escape = match (self.escape, c) {
                (Escape::None, '\x1b') => Escape::Start,
                (Escape::None, _) => {
                    self.draw(c, cols);
                    Escape::None
                }
                (Escape::Start, '[') => Escape::Control { bare: true },
                (Escape::Start, ']') => Escape::System,
                (Escape::Start, _) => {
                    match c {
                        '7' => self.saved = Some((self.row, self.col)),
                        '8' => (self.row, self.col) = self.saved.unwrap_or((self.row, self.col)),
                        _ => {}
                    }
                    Escape::None
                }
                (Escape::Control { bare }, '\x40'..='\x7e') => {
                    // The only one that moves the cursor among those the
                    // chat writes: to the first row, without parameters.
                    if bare && c == 'H' {
                        (self.row, self.col) = (0, 0);
                    }
                    Escape::None
                }
                (Escape::Control { .. }, _) => Escape::Control { bare: false },
                (Escape::System, '\x07') => Escape::None,
                // The escape of the `ESC \` that ends it.
                (Escape::System, '\x1b') => Escape::Start,
                (Escape::System, _) => Escape::System,
            };
        }
    }

    /// A character that is not of an escape sequence.
    fn draw(&mut self, c: char, cols: usize) {
        match c {
            '\n' => (self.row, self.col) = (self.row + 1, 0),
            '\r' => self.col = 0,
            '\t' => self.col = ((self.col / TAB_STOP + 1) * TAB_STOP).min(cols),
            _ => {
                let width = c.width().unwrap_or(0);
                // The terminal goes on at the start of the next row.
                if width > 0 && self.col + width > cols {
                    (self.row, self.col) = (self.row + 1, 0);
                }
                self.col += width;
            }
        }
        self.lowest = self.lowest.max(self.row);
    }
}

/// The screen as the chat's two streams and its line editor share it.
#[derive(Debug, Clone, Default)]
pub struct SharedScreen(Arc<Mutex<Screen>>);

impl SharedScreen {
    pub fn lock(&self) -> MutexGuard<'_, Screen> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A writer of the console whose output the screen follows, when it goes to
/// a terminal.
#[derive(Debug)]
pub struct Tracked<W> {
    writer: W,
    terminal: bool,
    screen: SharedScreen,
}

impl<W: ConsoleWriter> Tracked<W> {
    pub fn new(writer: W, screen: SharedScreen) -> Self {
        Self {
            terminal: writer.is_terminal(),
            writer,
            screen,
        }
    }
}

impl<W: ConsoleWriter> ConsoleWriter for Tracked<W> {
    fn write_str(&mut self, s: &str) -> Result<()> {
        if self.terminal {
            self.screen.lock().write(s);
        }
        self.writer.write_str(s)
    }

    fn flush(&mut self) -> Result<()> {
        self.writer.flush()
    }

    fn is_terminal(&self) -> bool {
        self.terminal
    }
}

#[cfg(test)]
mod tests {
    use super::{MARGIN, Screen};

    /// A screen of 20 columns by 10 rows that starts empty.
    fn screen() -> Screen {
        let mut screen = Screen::default();
        screen.start(20, 10);
        screen
    }

    /// The most lines that leave the first row where it is.
    const FITS: usize = 10 - 1 - MARGIN;

    #[test]
    fn the_first_row_holds_until_the_screen_is_full() {
        let mut screen = screen();
        assert!(screen.is_empty());
        screen.write(&"line\n".repeat(FITS));
        assert!(!screen.is_empty());
        assert!(screen.holds_first_row(20, 10));
        screen.write("one more\n");
        assert!(!screen.holds_first_row(20, 10));
        // For good: nothing brings back a row that has scrolled off.
        screen.write("\x1b[H");
        assert!(!screen.holds_first_row(20, 10));
    }

    #[test]
    fn long_lines_take_the_rows_the_terminal_wraps_them_on() {
        let mut screen = screen();
        // 45 columns on a screen of 20: three rows.
        screen.write(&format!("{}\n", "x".repeat(45)));
        screen.write(&"line\n".repeat(FITS - 3));
        assert!(screen.holds_first_row(20, 10));

        let mut screen = self::screen();
        screen.write(&format!("{}\n", "x".repeat(45)));
        screen.write(&"line\n".repeat(FITS - 2));
        assert!(!screen.holds_first_row(20, 10));
    }

    #[test]
    fn a_full_row_is_one_row_and_wide_characters_count_two_columns() {
        let mut screen = screen();
        // Exactly the width: the cursor waits at the end of the row.
        screen.write(&format!("{}\n", "x".repeat(20)));
        // Ten characters of two columns, and one more that wraps.
        screen.write(&format!("{}\n", "日".repeat(11)));
        screen.write(&"line\n".repeat(FITS - 3));
        assert!(screen.holds_first_row(20, 10));
        screen.write("\n");
        assert!(!screen.holds_first_row(20, 10));
    }

    #[test]
    fn escape_sequences_take_no_room() {
        let mut screen = screen();
        // Styles, a line erased and drawn again, a title for the window.
        let styled = "\x1b[1m\x1b[36m12345678901234567890\x1b[0m\n";
        screen.write(styled);
        screen.write("\r\x1b[Kpreview of a reply\r\x1b[K");
        screen.write("\x1b]0;a window title\x07\x1b]2;another\x1b\\");
        screen.write(&"line\n".repeat(FITS - 1));
        assert!(screen.holds_first_row(20, 10));
        // An escape sequence cut in two by the writer.
        screen.write("\x1b[3");
        screen.write("8;5;200mtext\n");
        assert!(!screen.holds_first_row(20, 10));
    }

    #[test]
    fn a_title_drawn_on_the_first_row_leaves_the_cursor_where_it_was() {
        let mut screen = screen();
        screen.write("header\nhint\n> ");
        // Save, go to the first row, draw a row and its line break, restore.
        screen.write(&format!("\x1b7\x1b[H\x1b[2K{}\n\x1b8", "-".repeat(20)));
        screen.write(&"\n".repeat(FITS - 2));
        assert!(screen.holds_first_row(20, 10));
        screen.write("\n");
        assert!(!screen.holds_first_row(20, 10));
        // A position is not the first row.
        let mut screen = self::screen();
        screen.write(&"\n".repeat(FITS));
        screen.write("\x1b[5;1H\n");
        assert!(!screen.holds_first_row(20, 10));
    }

    #[test]
    fn rows_drawn_on_and_erased_count_as_drawn() {
        let mut screen = screen();
        screen.write("header\n");
        // A menu of four rows under the prompt, gone when the line is
        // entered.
        screen.reach(4);
        screen.write("> /help\n");
        assert!(screen.holds_first_row(20, 10));
        screen.write(&"line\n".repeat(FITS - 5));
        assert!(screen.holds_first_row(20, 10));
        // The next prompt is too low for its menu.
        screen.reach(4);
        assert!(!screen.holds_first_row(20, 10));
    }

    #[test]
    fn another_size_or_an_unknown_screen_holds_nothing() {
        let mut screen = screen();
        screen.write("header\n");
        assert!(screen.holds_first_row(20, 10));
        assert!(!screen.holds_first_row(21, 10));
        assert!(!screen.holds_first_row(20, 30));
        screen.lose();
        assert!(!screen.holds_first_row(20, 10));
        assert!(!screen.is_empty());
        // What is written then is not followed, until the screen starts
        // empty again.
        screen.write("text\n");
        screen.start(20, 10);
        assert!(screen.is_empty());
        assert!(screen.holds_first_row(20, 10));
        // A screen that was never started is not known either.
        assert!(!Screen::default().holds_first_row(80, 24));
        assert!(!Screen::default().is_empty());
    }
}
