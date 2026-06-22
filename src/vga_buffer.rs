//! VGA text mode driver.
//!
//! VGA text mode exposes a memory region at physical address 0xb8000.
//! Writing bytes to that memory immediately changes what appears on the screen.
//!
//! Each screen cell occupies 2 bytes:
//
//! +------------+------------+
//! | ASCII byte | Color byte |
//! +------------+------------+
//
//! Example:
//!     'A' + white-on-black
//!
//! Since we are writing directly to hardware memory, no operating system,
//! graphics library, or system calls are involved.

use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use volatile::Volatile;

/// VGA supports 16 colors.
///
/// These values are not arbitrary Rust numbers.
/// The VGA hardware expects exactly these bit values.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)] // Store each enum variant as a single u8.
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

/// VGA stores foreground and background colors inside one byte.
///
/// Bit layout:
///
/// 7 6 5 4 | 3 2 1 0
/// --------+--------
///   BG    |   FG
///
/// Example:
///     White on Black:
///         background = 0000
///         foreground = 1111
///         result     = 00001111
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    /// Combine foreground and background colors into a VGA attribute byte.
    const fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8)) // upper 4 bits + lower 4 bits
    }
}

/// One cell in the VGA text buffer.
///
/// Memory layout:
///
/// +------+-------+
/// | byte | byte  |
/// +------+-------+
/// | 'A'  | color |
/// +------+-------+
///
/// repr(C) guarantees Rust won't reorder the fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

/// Complete VGA text buffer.
///
/// This represents: 25 rows × 80 columns
///
/// Each element is wrapped in `Volatile`.
/// Without volatile operations the compiler could conclude: "Nobody reads this memory."
/// and remove the writes entirely.
///
/// Hardware memory always requires volatile access
/// row 0   [0][0] [0][1] [0][2] ... [0][79]
//  row 1   [1][0] [1][1] [1][2] ... [1][79]
//  row 2   [2][0] [2][1] [2][2] ... [2][79]
//  ...
//  row 24  [24][0]    ...          [24][79]
#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

/// Responsible for writing characters to the screen.
///
/// The writer behaves similarly to a terminal:
///
/// - keeps track of cursor position
/// - moves to new lines
/// - scrolls when reaching the bottom
pub struct Writer {
    /// Current cursor column on the last screen row.
    column_position: usize,

    /// Current text color.
    color_code: ColorCode,

    /// Reference to the memory-mapped VGA buffer.
    /// Every write through this pointer directly modifies video memory.
    buffer: &'static mut Buffer,
}

impl Writer {
    /// Write a single byte to the screen.
    /// Newlines trigger scrolling logic.
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),

            byte => {
                // End of line reached.
                // Scroll the screen upward.
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                // Phil Opp writes only on the last row.
                // Earlier rows contain previous lines.
                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position;

                self.buffer.chars[row][col].write(ScreenChar {
                    ascii_character: byte,
                    color_code: self.color_code,
                });

                // Advance cursor.
                self.column_position += 1;
            }
        }
    }

    /// Write a Rust string.
    /// VGA only understands single-byte characters.
    /// UTF-8 characters like:
    ///     é
    ///     ह
    ///     你
    /// become multiple bytes and cannot be displayed.
    /// Unsupported bytes are replaced with ■.
    fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                _ => self.write_byte(0xfe),
            }
        }
    }

    /// Scroll the screen upward.
    /// Before:
    /// row 0  Hello
    /// row 1  World
    /// After:
    /// row 0  World
    /// row 1  ______
    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character =
                    self.buffer.chars[row][col].read();

                self.buffer.chars[row - 1][col]
                    .write(character);
            }
        }

        // Bottom row becomes empty.
        self.clear_row(BUFFER_HEIGHT - 1);

        // Cursor moves to beginning.
        self.column_position = 0;
    }

    /// Erase the entire screen.
    /// Every cell becomes:
    ///     ' ' + current color
    pub fn clear_screen(&mut self) {
        for row in 0..BUFFER_HEIGHT {
            self.clear_row(row);
        }
        self.column_position = 0;
    }

    /// Fill one row with spaces.
    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }

    // ==========================================================
    // Fixed-position drawing API
    // ==========================================================
    // Normal terminal output only writes at the cursor.
    // These methods allow direct screen access:
    //
    //      row,col
    //         ↓
    //     +---+---+---+
    //     | A | B | C |
    //     +---+---+---+
    //
    // Useful for:
    //
    // - menus
    // - status bars
    // - kernels
    // - dashboards
    // - box drawing characters

    pub const WIDTH: usize = BUFFER_WIDTH;
    pub const HEIGHT: usize = BUFFER_HEIGHT;

    /// Write exactly one cell.
    /// Unlike write_byte(), this does not:
    /// - move the cursor
    /// - scroll the screen
    /// - interpret newlines
    pub fn put_at(
        &mut self,
        row: usize,
        col: usize,
        byte: u8,
        fg: Color,
        bg: Color,
    ) {
        // Ignore invalid coordinates.
        if row >= BUFFER_HEIGHT || col >= BUFFER_WIDTH {
            return;
        }
        self.buffer.chars[row][col].write(ScreenChar {
            ascii_character: byte,
            color_code: ColorCode::new(fg, bg),
        });
    }

    /// Draw a string at a fixed location.
    /// Example:
    ///     put_str_at(0, 0, "Kernel", ...)
    /// places text in the upper-left corner.
    pub fn put_str_at(
        &mut self,
        row: usize,
        col: usize,
        s: &str,
        fg: Color,
        bg: Color,
    ) {
        for (i, byte) in s.bytes().enumerate() {
            self.put_at(row, col + i, byte, fg, bg);
        }
    }

    /// Paint the entire screen.
    /// Commonly used to create backgrounds.
    pub fn fill(
        &mut self,
        byte: u8,
        fg: Color,
        bg: Color,
    ) {
        for row in 0..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                self.put_at(row, col, byte, fg, bg);
            }
        }
    }
}

/// Allows:
///     write!(writer, "{}", x)
/// The formatting machinery eventually calls `write_str()`.
impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

lazy_static! {
    /// Global VGA writer.
    /// Why Mutex?
    /// Multiple CPUs or interrupts might print simultaneously.
    ///
    /// Why spin::Mutex?
    /// There is no scheduler yet.
    /// A normal mutex would put threads to sleep,
    /// but our kernel doesn't have threads.
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        column_position: 0,
        color_code: ColorCode::new(Color::LightGray, Color::Black),
        // SAFETY: VGA text memory always lives at 0xb8000.
        // We reinterpret that address as a Buffer.
        buffer: unsafe {
            &mut *(0xb8000 as *mut Buffer)
        },
    });
}

/// print!("hello")
/// Expands to:
///     _print(format_args!("hello"))
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => (
        $crate::vga_buffer::_print(
            format_args!($($arg)*)
        )
    );
}

/// println!("hello")
/// Expands to:
///     print!("hello\n")
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));

    ($($arg:tt)*) => (
        $crate::print!(
            "{}\n",
            format_args!($($arg)*)
        )
    );
}

/// Hidden implementation used by print!/println!.
/// The formatted string is sent to:
/// 1. VGA screen
/// 2. Serial port
///
/// Serial output allows logs to appear in QEMU even when the VGA
/// screen is unavailable.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    WRITER.lock().write_fmt(args).unwrap();
    crate::serial::_print(args);
}