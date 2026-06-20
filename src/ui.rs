//! On-screen clock interface: a centered, boxed panel drawn with positioned
//! writes (see `vga_buffer::Writer::put_at`). The static frame is drawn once;
//! the date/time lines are redrawn each time the second changes.

use crate::vga_buffer::{Color, Writer};
use core::fmt::Write;

// CP437 box-drawing glyphs (raw font bytes, not Unicode).
const TL: u8 = 0xC9; // ╔
const TR: u8 = 0xBB; // ╗
const BL: u8 = 0xC8; // ╚
const BR: u8 = 0xBC; // ╝
const HZ: u8 = 0xCD; // ═
const VT: u8 = 0xBA; // ║
const LJ: u8 = 0xCC; // ╠
const RJ: u8 = 0xB9; // ╣
const BULLET: u8 = 0x07; // •

// Panel geometry on the 80x25 grid.
const BOX_W: usize = 50;
const BOX_H: usize = 13;
const BOX_LEFT: usize = (Writer::WIDTH - BOX_W) / 2; // 15
const BOX_TOP: usize = (Writer::HEIGHT - BOX_H) / 2; // 6
const INNER_LEFT: usize = BOX_LEFT + 1;
const INNER_WIDTH: usize = BOX_W - 2;

// Row positions for each content line.
const ROW_TITLE: usize = BOX_TOP + 2;
const ROW_DIVIDER: usize = BOX_TOP + 3;
const ROW_DATE: usize = BOX_TOP + 5;
const ROW_TIME: usize = BOX_TOP + 7;
const ROW_ISO: usize = BOX_TOP + 9;
const ROW_FOOT: usize = BOX_TOP + 11;

const BG: Color = Color::Blue;

/// A tiny `core::fmt::Write` sink that formats into a fixed stack buffer, so we
/// can use `write!(...)` without a heap.
struct FmtBuf {
    buf: [u8; 80],
    len: usize,
}

impl FmtBuf {
    fn new() -> Self {
        FmtBuf { buf: [0; 80], len: 0 }
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl Write for FmtBuf {
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

/// Write `s` horizontally centered within the panel's inner width.
fn put_centered(w: &mut Writer, row: usize, s: &str, fg: Color) {
    let len = s.len();
    let col = if len < INNER_WIDTH {
        INNER_LEFT + (INNER_WIDTH - len) / 2
    } else {
        INNER_LEFT
    };
    w.put_str_at(row, col, s, fg, BG);
}

/// Blank one inner row back to the background (so shorter text leaves no
/// leftovers when we redraw).
fn clear_inner_row(w: &mut Writer, row: usize) {
    for col in INNER_LEFT..(INNER_LEFT + INNER_WIDTH) {
        w.put_at(row, col, b' ', Color::White, BG);
    }
}

/// Draw the box outline.
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

/// Draw everything that never changes: background, box, title, divider.
pub fn draw_frame(w: &mut Writer) {
    w.fill(b' ', Color::White, BG);
    draw_box(w, Color::LightCyan);

    // Divider line under the title (╠══════╣).
    let right = BOX_LEFT + BOX_W - 1;
    w.put_at(ROW_DIVIDER, BOX_LEFT, LJ, Color::LightCyan, BG);
    w.put_at(ROW_DIVIDER, right, RJ, Color::LightCyan, BG);
    for c in (BOX_LEFT + 1)..right {
        w.put_at(ROW_DIVIDER, c, HZ, Color::LightCyan, BG);
    }

    put_centered(w, ROW_TITLE, "BARE-METAL  RTC  CLOCK", Color::White);
}

/// Redraw the date/time lines. `blink` toggles a small heartbeat each second.
pub fn draw_dynamic(w: &mut Writer, dt: &crate::rtc::DateTime, blink: bool) {
    let weekday = crate::rtc::weekday_full(crate::rtc::day_of_week(dt));

    // Date line: "Friday, 19 June 2026"
    clear_inner_row(w, ROW_DATE);
    let mut date = FmtBuf::new();
    let _ = write!(
        date,
        "{}, {} {} {}",
        weekday,
        dt.day,
        crate::rtc::month_full(dt.month),
        dt.year
    );
    put_centered(w, ROW_DATE, date.as_str(), Color::Yellow);

    // Big time line: "15 : 49 : 21" (constant width, bright).
    let mut time = FmtBuf::new();
    let _ = write!(
        time,
        "{:02} : {:02} : {:02}",
        dt.hour, dt.minute, dt.second
    );
    put_centered(w, ROW_TIME, time.as_str(), Color::LightGreen);

    // ISO-8601 line.
    let mut iso = FmtBuf::new();
    let _ = write!(
        iso,
        "{}-{:02}-{:02}T{:02}:{:02}:{:02}",
        dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second
    );
    put_centered(w, ROW_ISO, iso.as_str(), Color::LightGray);

    // Footer with a blinking heartbeat dot to show the clock is live.
    clear_inner_row(w, ROW_FOOT);
    let label = "live  reading CMOS RTC @ 0x70/0x71 ";
    let col = INNER_LEFT + (INNER_WIDTH - (label.len() + 1)) / 2;
    w.put_str_at(ROW_FOOT, col, label, Color::LightGray, BG);
    let dot_color = if blink { Color::LightGreen } else { BG };
    w.put_at(ROW_FOOT, col + label.len(), BULLET, dot_color, BG);
}
