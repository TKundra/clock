//! Simple text-mode clock UI.
//
//! Unlike `println!`, this module does not use the terminal cursor.
//! Instead it draws directly at fixed screen coordinates.
//
//! The UI consists of:
//
//!     +------------------------------------------+
//!     |          BARE-METAL RTC CLOCK            |
//!     |------------------------------------------|
//!     |                                          |
//!     |         Friday, 19 June 2026             |
//!     |                                          |
//!     |            15 : 49 : 21                  |
//!     |                                          |
//!     |          2026-06-19T15:49:21             |
//!     |                                          |
//!     |    live reading CMOS RTC @ 0x70/0x71 •   |
//!     +------------------------------------------+
//
//! The frame is drawn once.
//! The changing text is redrawn every second.

use crate::vga_buffer::{Color, Writer};
use core::fmt::Write;

// ======================================================
// CP437 box drawing characters.
//
// VGA text mode does not understand Unicode.
//
// These values are raw bytes in the VGA font ROM:
//
//     0xC9 = ╔
//     0xBB = ╗
//     0xC8 = ╚
//     0xBC = ╝
//
// Writing these bytes directly produces box graphics.
// ======================================================

const TL: u8 = 0xC9; // top-left corner
const TR: u8 = 0xBB; // top-right corner
const BL: u8 = 0xC8; // bottom-left corner
const BR: u8 = 0xBC; // bottom-right corner

const HZ: u8 = 0xCD; // horizontal line
const VT: u8 = 0xBA; // vertical line

const LJ: u8 = 0xCC; // left junction
const RJ: u8 = 0xB9; // right junction

const BULLET: u8 = 0x07; // bullet character

// ======================================================
// Screen layout.
//
// VGA text mode is:
//     80 columns × 25 rows
//
// The clock panel is centered on that grid.
// ======================================================

const BOX_W: usize = 50;
const BOX_H: usize = 13;

// Compute the top-left corner.
//
// (80 - 50) / 2 = 15
// (25 - 13) / 2 = 6
//
// This centers the box.
const BOX_LEFT: usize = (Writer::WIDTH - BOX_W) / 2;
const BOX_TOP: usize = (Writer::HEIGHT - BOX_H) / 2;

// Interior area (excluding borders).
const INNER_LEFT: usize = BOX_LEFT + 1;
const INNER_WIDTH: usize = BOX_W - 2;

// ======================================================
// Content rows.
//
// Each piece of information always appears on a fixed row.
// ======================================================

const ROW_TITLE: usize = BOX_TOP + 2;
const ROW_DIVIDER: usize = BOX_TOP + 3;
const ROW_DATE: usize = BOX_TOP + 5;
const ROW_TIME: usize = BOX_TOP + 7;
const ROW_ISO: usize = BOX_TOP + 9;
const ROW_FOOT: usize = BOX_TOP + 11;

// Blue panel background.
const BG: Color = Color::Blue;

/// Draw text centered within the box.
/// Example:
///     width = 48
///     text  = 20
///     left padding = 14
fn put_centered(
    w: &mut Writer,
    row: usize,
    s: &str,
    fg: Color,
) {
    let len = s.len();

    let col =
        if len < INNER_WIDTH {
            INNER_LEFT +
                (INNER_WIDTH - len) / 2
        } else {
            INNER_LEFT
        };

    w.put_str_at(row, col, s, fg, BG);
}

/// Erase one interior row.
/// This is important because: "15:59:59"
/// later becoming: "9:00:00"
///
/// would otherwise leave old characters behind.
fn clear_inner_row(
    w: &mut Writer,
    row: usize,
) {
    for col in INNER_LEFT..
        (INNER_LEFT + INNER_WIDTH)
    {
        w.put_at(
            row,
            col,
            b' ',
            Color::White,
            BG,
        );
    }
}

/// Draw the border.
/// Produces:
///
///     ╔════════════╗
///     ║            ║
///     ╚════════════╝
fn draw_box(
    w: &mut Writer,
    fg: Color,
) {
    let right = BOX_LEFT + BOX_W - 1;
    let bottom = BOX_TOP + BOX_H - 1;

    // Corners.
    w.put_at(BOX_TOP, BOX_LEFT, TL, fg, BG);
    w.put_at(BOX_TOP, right, TR, fg, BG);

    w.put_at(bottom, BOX_LEFT, BL, fg, BG);
    w.put_at(bottom, right, BR, fg, BG);

    // Horizontal borders.
    for c in (BOX_LEFT + 1)..right {
        w.put_at(BOX_TOP, c, HZ, fg, BG);
        w.put_at(bottom, c, HZ, fg, BG);
    }

    // Vertical borders.
    for r in (BOX_TOP + 1)..bottom {
        w.put_at(r, BOX_LEFT, VT, fg, BG);
        w.put_at(r, right, VT, fg, BG);
    }
}

/// Draw everything that never changes.
///
/// This function is called once:
///
/// - background
/// - border
/// - divider
/// - title
///
/// Dynamic content is drawn separately.
pub fn draw_frame(w: &mut Writer) {
    // Paint the entire screen blue.
    w.fill(b' ', Color::White, BG);

    draw_box(w, Color::LightCyan);

    // Title separator:
    //
    //     ╠══════════╣
    let right = BOX_LEFT + BOX_W - 1;

    w.put_at(
        ROW_DIVIDER,
        BOX_LEFT,
        LJ,
        Color::LightCyan,
        BG,
    );

    w.put_at(
        ROW_DIVIDER,
        right,
        RJ,
        Color::LightCyan,
        BG,
    );

    for c in (BOX_LEFT + 1)..right {
        w.put_at(
            ROW_DIVIDER,
            c,
            HZ,
            Color::LightCyan,
            BG,
        );
    }

    put_centered(
        w,
        ROW_TITLE,
        "BARE-METAL RTC CLOCK",
        Color::White,
    );
}

/// Small formatting buffer.
///
/// The kernel currently has no heap allocation,
/// so we cannot use: String::new()
///
/// Instead we reserve 80 bytes on the stack.
/// The standard formatting macros: write!(...)
///
/// can write into this fixed buffer.
struct FmtBuf {
    buf: [u8; 80],
    len: usize,
}

impl FmtBuf {
    fn new() -> Self {
        FmtBuf {
            buf: [0; 80],
            len: 0,
        }
    }

    /// Interpret the written bytes as UTF-8 text.
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

/// Allow:  write!(buffer, "{}", value)
/// The formatting machinery repeatedly calls write_str().
impl Write for FmtBuf {
    fn write_str(
        &mut self,
        s: &str,
    ) -> core::fmt::Result {

        for &b in s.as_bytes() {
            // Prevent buffer overflow.
            if self.len < self.buf.len() {
                self.buf[self.len] = b;
                self.len += 1;
            }
        }

        Ok(())
    }
}



/// Redraw the changing data.
///
/// Called once every second.
///
/// Only dynamic rows are updated:
///
/// - date
/// - time
/// - ISO timestamp
// - heartbeat indicator
pub fn draw_dynamic(
    w: &mut Writer,
    dt: &crate::rtc::DateTime,
    blink: bool,
) {
    let weekday =
        crate::rtc::weekday_full(
            crate::rtc::day_of_week(dt)
        );

    // ==================================================
    // Date:
    //
    //     Friday, 19 June 2026
    // ==================================================

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

    put_centered(
        w,
        ROW_DATE,
        date.as_str(),
        Color::Yellow,
    );

    // ==================================================
    // Time:
    //
    //     15 : 49 : 21
    // ==================================================

    let mut time = FmtBuf::new();

    let _ = write!(
        time,
        "{:02} : {:02} : {:02}",
        dt.hour,
        dt.minute,
        dt.second
    );

    put_centered(
        w,
        ROW_TIME,
        time.as_str(),
        Color::LightGreen,
    );

    // ==================================================
    // ISO-8601 timestamp.
    // ==================================================

    let mut iso = FmtBuf::new();

    let _ = write!(
        iso,
        "{}-{:02}-{:02}T{:02}:{:02}:{:02}",
        dt.year,
        dt.month,
        dt.day,
        dt.hour,
        dt.minute,
        dt.second
    );

    put_centered(
        w,
        ROW_ISO,
        iso.as_str(),
        Color::LightGray,
    );

    // ==================================================
    // Heartbeat indicator.
    //
    // The blinking dot proves the kernel is still
    // running and updating.
    // ==================================================

    clear_inner_row(w, ROW_FOOT);

    let label =
        "live reading CMOS RTC @ 0x70/0x71 ";

    let col =
        INNER_LEFT +
            (INNER_WIDTH - (label.len() + 1)) / 2;

    w.put_str_at(
        ROW_FOOT,
        col,
        label,
        Color::LightGray,
        BG,
    );

    let dot_color =
        if blink {
            Color::LightGreen
        } else {
            BG
        };

    w.put_at(
        ROW_FOOT,
        col + label.len(),
        BULLET,
        dot_color,
        BG,
    );
}