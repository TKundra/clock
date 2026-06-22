# Chapter 07 — A Realtime Clock with a UI

In Chapter 06 we read the time **once** and printed it. A real clock *ticks*.
This chapter turns the one-shot `date` into a live, self-updating clock with a
proper on-screen interface: a centered panel that redraws every second.

We'll do it **without interrupts** — a deliberate choice. The "correct" OS way
is a timer interrupt (Chapter 09 covers it), but that's a big leap. A polling
loop that re-reads the RTC and repaints only when the second changes is simple,
robust, and genuinely realtime. We'll layer interrupts on top later.

## Two ingredients

1. **Positioned drawing.** Our `println!` scrolls from the bottom — useless for a
   clock that updates in place. We need to write a glyph at an exact `(row,
   col)`. We added that API in Chapter 03 (`put_at`, `put_str_at`, `fill`).
2. **A redraw-on-tick loop.** Poll `rtc::read()`, compare to the last value, and
   only repaint when it changes (so the screen never flickers and we don't burn
   the CPU repainting identical frames).

## Step 1: full-name helpers in `rtc.rs`

The UI shows "Friday, 19 June 2026", so add full-name lookups next to the
abbreviated ones from Chapter 05:

```rust
// src/rtc.rs
pub fn weekday_full(index: u8) -> &'static str {
    const NAMES: [&str; 7] = [
        "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday",
    ];
    NAMES[(index % 7) as usize]
}

pub fn month_full(month: u8) -> &'static str {
    const NAMES: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August",
        "September", "October", "November", "December",
    ];
    NAMES[((month.saturating_sub(1)) % 12) as usize]
}
```

## Step 2: a serial-only tick log

For headless verification (and as a log that doesn't disturb the UI), add a
serial-only `println` to `src/serial.rs`:

```rust
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial::_print(format_args!("\n")));
    ($($arg:tt)*) => ($crate::serial::_print(format_args!("{}\n", format_args!($($arg)*))));
}
```

Each tick prints one line to the serial port. Run QEMU with `-serial stdio` and
you get a ticking log — which is exactly how we'll prove the clock advances in
real time in Chapter 08.

## Step 3: formatting without a heap

We want `write!(…, "{:02}", n)` to build strings, but there's no `String` in
`no_std`. The fix is a tiny sink that implements `core::fmt::Write` into a
fixed stack buffer:

```rust
// src/ui.rs
struct FmtBuf {
    buf: [u8; 80],
    len: usize,
}

impl FmtBuf {
    fn new() -> Self { FmtBuf { buf: [0; 80], len: 0 } }
    fn as_str(&self) -> &str { core::str::from_utf8(&self.buf[..self.len]).unwrap_or("") }
}

impl core::fmt::Write for FmtBuf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for &b in s.as_bytes() {
            if self.len < self.buf.len() {
                self.buf[self.len] = b;
                self.len += 1;
            }
        }
        Ok(())
    }
}
```

Now `write!(&mut buf, "…")` works, and `buf.as_str()` hands us the result to
draw. (This pattern is worth remembering — it's how you do *any* formatting in a
kernel without a heap.)

## Step 4: the UI module

The interface is a centered box drawn with **CP437 box-drawing glyphs**. VGA
text mode uses code page 437, where bytes like `0xC9` render as `╔`. Our
`write_string` filter (Chapter 03) rejects non-ASCII, which is exactly why we
added the raw `put_at` — it writes the byte straight through.

### Geometry and glyphs

```rust
// src/ui.rs
use crate::vga_buffer::{Color, Writer};
use core::fmt::Write;

// CP437 box-drawing glyphs (raw font bytes, not Unicode).
const TL: u8 = 0xC9; const TR: u8 = 0xBB;   // ╔ ╗
const BL: u8 = 0xC8; const BR: u8 = 0xBC;   // ╚ ╝
const HZ: u8 = 0xCD; const VT: u8 = 0xBA;   // ═ ║
const LJ: u8 = 0xCC; const RJ: u8 = 0xB9;   // ╠ ╣
const BULLET: u8 = 0x07;                    // •

// Panel geometry on the 80x25 grid (centered).
const BOX_W: usize = 50;
const BOX_H: usize = 13;
const BOX_LEFT: usize = (Writer::WIDTH - BOX_W) / 2;  // 15
const BOX_TOP: usize = (Writer::HEIGHT - BOX_H) / 2;  // 6
const INNER_LEFT: usize = BOX_LEFT + 1;
const INNER_WIDTH: usize = BOX_W - 2;

const ROW_TITLE: usize = BOX_TOP + 2;
const ROW_DIVIDER: usize = BOX_TOP + 3;
const ROW_DATE: usize = BOX_TOP + 5;
const ROW_TIME: usize = BOX_TOP + 7;
const ROW_ISO: usize = BOX_TOP + 9;
const ROW_FOOT: usize = BOX_TOP + 11;

const BG: Color = Color::Blue;
```

### Helpers: centering and clearing

```rust
fn put_centered(w: &mut Writer, row: usize, s: &str, fg: Color) {
    let len = s.len();
    let col = if len < INNER_WIDTH {
        INNER_LEFT + (INNER_WIDTH - len) / 2
    } else {
        INNER_LEFT
    };
    w.put_str_at(row, col, s, fg, BG);
}

// Blank one inner row so shorter new text leaves no leftovers on redraw.
fn clear_inner_row(w: &mut Writer, row: usize) {
    for col in INNER_LEFT..(INNER_LEFT + INNER_WIDTH) {
        w.put_at(row, col, b' ', Color::White, BG);
    }
}
```

> **Why `clear_inner_row` matters:** when the date changes from
> "…9 June…" to "…19 June…" the new text is longer/shorter. Positioned writes
> only overwrite the cells they touch, so without clearing first you'd see
> stale characters from the previous frame. The time line is constant-width
> (always 2 digits) so it doesn't strictly need it, but the date line does.

### The box and the static frame

```rust
fn draw_box(w: &mut Writer, fg: Color) {
    let right = BOX_LEFT + BOX_W - 1;
    let bottom = BOX_TOP + BOX_H - 1;
    w.put_at(BOX_TOP, BOX_LEFT, TL, fg, BG);
    w.put_at(BOX_TOP, right, TR, fg, BG);
    w.put_at(bottom, BOX_LEFT, BL, fg, BG);
    w.put_at(bottom, right, BR, fg, BG);
    for c in (BOX_LEFT + 1)..right {
        w.put_at(BOX_TOP, c, HZ, fg, BG);
        w.put_at(bottom, c, HZ, fg, BG);
    }
    for r in (BOX_TOP + 1)..bottom {
        w.put_at(r, BOX_LEFT, VT, fg, BG);
        w.put_at(r, right, VT, fg, BG);
    }
}

/// Everything that never changes: background, box, title, divider.
pub fn draw_frame(w: &mut Writer) {
    w.fill(b' ', Color::White, BG);
    draw_box(w, Color::LightCyan);

    let right = BOX_LEFT + BOX_W - 1;
    w.put_at(ROW_DIVIDER, BOX_LEFT, LJ, Color::LightCyan, BG);
    w.put_at(ROW_DIVIDER, right, RJ, Color::LightCyan, BG);
    for c in (BOX_LEFT + 1)..right {
        w.put_at(ROW_DIVIDER, c, HZ, Color::LightCyan, BG);
    }

    put_centered(w, ROW_TITLE, "BARE-METAL  RTC  CLOCK", Color::White);
}
```

### The dynamic part (redrawn each second)

```rust
/// Redraw date/time. `blink` toggles a heartbeat dot each second.
pub fn draw_dynamic(w: &mut Writer, dt: &crate::rtc::DateTime, blink: bool) {
    let weekday = crate::rtc::weekday_full(crate::rtc::day_of_week(dt));

    // Date: "Friday, 19 June 2026"
    clear_inner_row(w, ROW_DATE);
    let mut date = FmtBuf::new();
    let _ = write!(date, "{}, {} {} {}",
        weekday, dt.day, crate::rtc::month_full(dt.month), dt.year);
    put_centered(w, ROW_DATE, date.as_str(), Color::Yellow);

    // Big time: "15 : 49 : 21"
    let mut time = FmtBuf::new();
    let _ = write!(time, "{:02} : {:02} : {:02}", dt.hour, dt.minute, dt.second);
    put_centered(w, ROW_TIME, time.as_str(), Color::LightGreen);

    // ISO-8601 line
    let mut iso = FmtBuf::new();
    let _ = write!(iso, "{}-{:02}-{:02}T{:02}:{:02}:{:02}",
        dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second);
    put_centered(w, ROW_ISO, iso.as_str(), Color::LightGray);

    // Footer with a blinking heartbeat dot to show the clock is live.
    clear_inner_row(w, ROW_FOOT);
    let label = "live  reading CMOS RTC @ 0x70/0x71 ";
    let col = INNER_LEFT + (INNER_WIDTH - (label.len() + 1)) / 2;
    w.put_str_at(ROW_FOOT, col, label, Color::LightGray, BG);
    let dot_color = if blink { Color::LightGreen } else { BG };
    w.put_at(ROW_FOOT, col + label.len(), BULLET, dot_color, BG);
}
```

## Step 5: the realtime loop in `main.rs`

Replace the one-shot `_start`/`cmd_date` from Chapter 06 with a draw-once-then-
loop structure:

```rust
#![no_std]
#![no_main]

mod rtc;
mod serial;
mod ui;          // <-- new module
mod vga_buffer;

use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    serial_println!("clock kernel — live bare-metal RTC clock");

    // Draw the static UI once, then run the live clock loop forever.
    {
        let mut writer = vga_buffer::WRITER.lock();
        ui::draw_frame(&mut writer);
    }

    run_clock()
}

/// Poll the RTC and redraw only when the second changes.
fn run_clock() -> ! {
    let mut last: Option<rtc::DateTime> = None;
    let mut blink = false;

    loop {
        let now = rtc::read();

        if Some(now) != last {
            blink = !blink;

            {
                let mut writer = vga_buffer::WRITER.lock();
                ui::draw_dynamic(&mut writer, &now, blink);
            }

            serial_println!(
                "{}-{:02}-{:02}T{:02}:{:02}:{:02}",
                now.year, now.month, now.day, now.hour, now.minute, now.second
            );

            last = Some(now);
        }

        // Tiny pause so we don't hammer the CMOS ports between ticks.
        for _ in 0..50_000 {
            core::hint::spin_loop();
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("KERNEL PANIC: {}", info);
    loop { x86_64::instructions::hlt(); }
}
```

### Why this design

- **`Some(now) != last`** — `DateTime` derives `PartialEq` (Chapter 05), so we
  detect a tick by simple equality. We only lock the screen and repaint on a
  *change*, so the display is flicker-free and idle frames are cheap.
- **The inner lock scopes `{ … }`** — we grab the `WRITER` lock, draw, and drop
  it immediately. Holding a spinlock longer than necessary is a classic kernel
  bug; tight scopes keep us honest (and matter once interrupts exist).
- **The `spin_loop()` pause** — `rtc::read()` busy-waits on the
  update-in-progress flag, so calling it in a tight loop is wasteful. A short
  spin between polls is a crude "good enough" throttle. (With a timer interrupt
  you'd `hlt` until woken instead — Chapter 09.)
- **No `hlt` halt loop at the end** — `run_clock` never returns, so `_start`'s
  body ends there. The clock runs until you close QEMU.

## Step 6: build and watch it tick

```bash
cargo run
```

You should see a centered panel with the title, the full date, a big
`HH : MM : SS` that advances every second, the ISO line, and a heartbeat dot
that blinks each tick. Chapter 08 covers verifying this headlessly.

## Checkpoint

The clock is now live: a drawn UI that updates in real time by polling the RTC.
You've learned positioned VGA drawing, heap-free formatting, and the
poll-redraw-on-change pattern. Next: build it into an image, boot it, and prove
the ticking is real.

---

Prev: [Chapter 06 — The `date` command](06-date-command.md) ·
Next: [Chapter 08 — Build, boot & verify →](08-build-boot-verify.md)
