# Chapter 03 — VGA Text Buffer

Goal: print text to the screen. We build a `vga_buffer` module and `println!` /
`print!` macros that work without an OS.

## The idea: the screen *is* memory

In VGA text mode the display is an 80×25 grid of cells living at physical
address `0xb8000`. Each cell is **two bytes**:

```
byte 0: ASCII code point   (e.g. 0x41 = 'A')
byte 1: color attribute    (high nibble = background, low nibble = foreground)
```

So cell `(row, col)` lives at `0xb8000 + (row * 80 + col) * 2`. Write a byte
there and the character appears instantly. We don't ask anyone's permission —
we're the only thing running.

## Step 1: add dependencies

```toml
# Cargo.toml  [dependencies]
volatile = "0.2.6"
spin = "0.5.2"
lazy_static = { version = "1.0", features = ["spin_no_std"] }
```

- **`volatile`** — wraps each cell so the compiler can't "optimize away" our
  writes. It thinks they're dead stores (we never read them back); `volatile`
  tells it they have a side effect it can't see.
- **`spin`** — a spinlock. We want a *global* screen writer, which means shared
  mutable state, which needs a lock. With no OS there's no real mutex to block
  on, so we spin.
- **`lazy_static`** — lets us initialize that global *the first time it's used*
  (we can't compute `&mut *(0xb8000 …)` in a `const`). The `spin_no_std` feature
  makes it work without std.

## Step 2: colors and a cell type

```rust
// src/vga_buffer.rs
use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use volatile::Volatile;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0, Blue = 1, Green = 2, Cyan = 3,
    Red = 4, Magenta = 5, Brown = 6, LightGray = 7,
    DarkGray = 8, LightBlue = 9, LightGreen = 10, LightCyan = 11,
    LightRed = 12, Pink = 13, Yellow = 14, White = 15,
}

/// Foreground + background packed into the single attribute byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    const fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

/// One on-screen cell. `repr(C)` guarantees ascii-then-color byte order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

/// The memory-mapped buffer itself.
#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}
```

`#[repr(C)]` on `ScreenChar` and `#[repr(transparent)]` on the wrappers matter:
the in-memory layout must *exactly* match what the hardware reads, so we can't
let Rust reorder fields.

## Step 3: the Writer

The `Writer` tracks the current column and always writes to the bottom row,
scrolling up when needed (a simple terminal model).

```rust
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
                0x20..=0x7e | b'\n' => self.write_byte(byte), // printable + newline
                _ => self.write_byte(0xfe),                   // others → ■
            }
        }
    }

    fn new_line(&mut self) {
        // Scroll: copy every row up by one, drop the top row.
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer.chars[row][col].read();
                self.buffer.chars[row - 1][col].write(character);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    /// Blank the whole screen and home the cursor.
    pub fn clear_screen(&mut self) {
        for row in 0..BUFFER_HEIGHT {
            self.clear_row(row);
        }
        self.column_position = 0;
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar { ascii_character: b' ', color_code: self.color_code };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }
}
```

Note `write_string` only emits printable ASCII (and `\n`). A `&str` is UTF-8, so
a stray multi-byte character would otherwise dump nonsense; we replace anything
outside `0x20..=0x7e` with `0xfe` (a `■`).

## Step 4: make `write!` work — impl `fmt::Write`

To use Rust's formatting machinery (`write!`, `{:02}`, etc.) we implement
`core::fmt::Write`. That single method unlocks all of `format_args!`.

```rust
impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}
```

## Step 5: the global writer

```rust
lazy_static! {
    /// The one global screen writer, behind a spinlock.
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        column_position: 0,
        color_code: ColorCode::new(Color::LightGray, Color::Black),
        // SAFETY: 0xb8000 is the fixed, identity-mapped VGA buffer; we're the
        // only writer to it.
        buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
    });
}
```

That `unsafe` cast is the one genuinely unsafe line: we promise the compiler
that `0xb8000` is a valid, exclusively-owned `Buffer`. It is, because the
bootloader identity-maps low memory and nothing else touches it.

## Step 6: the `print!` / `println!` macros

```rust
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
}
```

These mirror the standard library's macros, but route through *our* `_print`,
which locks the global writer and feeds it the formatted arguments.

## Step 7: use it from `_start`

Wire the module into `main.rs` and print something:

```rust
// src/main.rs
#![no_std]
#![no_main]

mod vga_buffer;          // <-- add

use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    vga_buffer::WRITER.lock().clear_screen();
    println!("Hello from bare metal!");
    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("KERNEL PANIC: {}", info); // now we can print panics!
    loop {}
}
```

We can't *run* it on screen until Chapter 08 (we need `bootimage` + QEMU), but
it compiles, and the panic handler can now report failures.

## Step 8: a positioned-drawing API (for the UI later)

`println!` scrolls from the bottom row — perfect for log-style output, useless
for a clock that updates *in place*. In Chapter 07 we'll draw a fixed UI, which
needs two things `write_string` can't do: write at an exact `(row, col)`, and
emit **raw CP437 bytes** (box-drawing glyphs like `╔` = `0xC9`) that the ASCII
filter would otherwise reject.

So add a small positioned API to `impl Writer` (it bypasses the cursor and
scrolling entirely):

```rust
    pub const WIDTH: usize = BUFFER_WIDTH;
    pub const HEIGHT: usize = BUFFER_HEIGHT;

    /// Paint one cell with a raw code-page-437 byte and an explicit color.
    pub fn put_at(&mut self, row: usize, col: usize, byte: u8, fg: Color, bg: Color) {
        if row >= BUFFER_HEIGHT || col >= BUFFER_WIDTH {
            return; // ignore out-of-bounds coordinates
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
```

These don't touch `column_position` — they're pure "paint this cell" operations.
The bounds check means a bad coordinate is silently ignored rather than panicking
out of the screen buffer. We won't use them until Chapter 07; they're here
because they belong with the rest of the VGA code.

## Checkpoint

You have working `println!` plus a positioned-drawing API for later. Everything
from here can report what it's doing. Next we add a serial console — the
kernel-dev debugging workhorse — so we can capture output as *text* even with no
display.

---

Prev: [Chapter 02 — Freestanding binary](02-freestanding-binary.md) ·
Next: [Chapter 04 — Serial port output →](04-serial-port.md)
