//! Minimal driver for the VGA text-mode buffer at physical address 0xb8000.
//!
//! In text mode the screen is a 80x25 grid. Each cell is two bytes: an ASCII
//! code point and a color attribute byte. Writing into this memory-mapped
//! buffer immediately changes what is on screen — there is no syscall here, we
//! are the only thing running on the machine.

use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use volatile::Volatile;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

/// A foreground/background pair packed into the single attribute byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    const fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

/// One character cell on screen: ASCII byte + color. `repr(C)` guarantees the
/// field order the hardware expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

/// The memory-mapped buffer. `Volatile` keeps the compiler from "optimizing
/// away" writes it thinks are dead — they have a side effect it can't see.
#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

/// Tracks the cursor column and writes characters, scrolling when it hits the
/// bottom of the screen.
pub struct Writer {
    column_position: usize,
    color_code: ColorCode,
    buffer: &'static mut Buffer,
}

impl Writer {
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position;
                let color_code = self.color_code;
                self.buffer.chars[row][col].write(ScreenChar {
                    ascii_character: byte,
                    color_code,
                });
                self.column_position += 1;
            }
        }
    }

    fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                // Printable ASCII range, plus newline.
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                // Anything else (e.g. non-ASCII UTF-8 bytes) → ■
                _ => self.write_byte(0xfe),
            }
        }
    }

    fn new_line(&mut self) {
        // Scroll every row up by one, dropping the top row.
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer.chars[row][col].read();
                self.buffer.chars[row - 1][col].write(character);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    /// Blank the whole screen and move the cursor to the top-left.
    #[allow(dead_code)] // handy utility; the UI uses `fill` instead
    pub fn clear_screen(&mut self) {
        for row in 0..BUFFER_HEIGHT {
            self.clear_row(row);
        }
        self.column_position = 0;
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }

    // --- Positioned drawing API ---------------------------------------------
    // These bypass the cursor/scrolling model above. They write a raw byte at
    // an exact (row, col), which lets us draw a fixed UI and use CP437 glyphs
    // (box-drawing characters, bullets) that the `write_string` ASCII filter
    // would otherwise reject. Out-of-bounds coordinates are ignored.

    pub const WIDTH: usize = BUFFER_WIDTH;
    pub const HEIGHT: usize = BUFFER_HEIGHT;

    /// Paint one cell with a raw code-page-437 byte and an explicit color.
    pub fn put_at(&mut self, row: usize, col: usize, byte: u8, fg: Color, bg: Color) {
        if row >= BUFFER_HEIGHT || col >= BUFFER_WIDTH {
            return;
        }
        self.buffer.chars[row][col].write(ScreenChar {
            ascii_character: byte,
            color_code: ColorCode::new(fg, bg),
        });
    }

    /// Write an ASCII string starting at (row, col), left-to-right.
    pub fn put_str_at(&mut self, row: usize, col: usize, s: &str, fg: Color, bg: Color) {
        for (i, byte) in s.bytes().enumerate() {
            self.put_at(row, col + i, byte, fg, bg);
        }
    }

    /// Fill the whole screen with one glyph/color (used to paint a background).
    pub fn fill(&mut self, byte: u8, fg: Color, bg: Color) {
        for row in 0..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                self.put_at(row, col, byte, fg, bg);
            }
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

lazy_static! {
    /// The one global screen writer. A spinlock stands in for a real mutex
    /// because there is no OS scheduler to block on.
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        column_position: 0,
        color_code: ColorCode::new(Color::LightGray, Color::Black),
        // SAFETY: 0xb8000 is the fixed, identity-mapped VGA text buffer and we
        // are the only writer to it.
        buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
    });
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    WRITER.lock().write_fmt(args).unwrap();
    // Mirror everything to the serial console so headless QEMU shows it too.
    crate::serial::_print(args);
}
