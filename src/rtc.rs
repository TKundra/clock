//! Driver for the CMOS Real-Time Clock (the "RTC" — historically the
//! Motorola MC146818 and its descendants, today part of the chipset).
//!
//! The RTC keeps wall-clock time even while the machine is off, powered by the
//! coin battery on the motherboard. We talk to it through two I/O ports:
//!
//!   * port 0x70 — the *index* port: write the register number you want.
//!   * port 0x71 — the *data* port:  then read/write that register's value.
//!
//! The clock registers (seconds, minutes, ...) live at fixed indices. Two
//! gotchas make this more than "read six bytes":
//!
//!   1. **Updates**: roughly once a second the chip updates its registers. If
//!      you read mid-update you can get garbage, so we wait for the
//!      "update in progress" flag and read twice to confirm a stable value.
//!   2. **BCD**: by default values are stored in Binary-Coded Decimal (the
//!      byte 0x59 means the decimal number 59, not 0x59 = 89). Status
//!      register B tells us whether that's the case, and whether hours are
//!      12- or 24-hour.

use x86_64::instructions::port::Port;

const CMOS_INDEX_PORT: u16 = 0x70;
const CMOS_DATA_PORT: u16 = 0x71;

// RTC register indices.
const REG_SECONDS: u8 = 0x00;
const REG_MINUTES: u8 = 0x02;
const REG_HOURS: u8 = 0x04;
const REG_DAY: u8 = 0x07;
const REG_MONTH: u8 = 0x08;
const REG_YEAR: u8 = 0x09;
const REG_STATUS_A: u8 = 0x0A;
const REG_STATUS_B: u8 = 0x0B;

/// A point in time as reported by the RTC, already decoded to plain binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// Read one CMOS register.
///
/// The high bit of the index port also controls the NMI-disable line; writing
/// the bare register index (high bit clear) leaves NMIs enabled, which is what
/// we want.
fn read_register(reg: u8) -> u8 {
    // SAFETY: ports 0x70/0x71 are the standardized CMOS index/data ports.
    unsafe {
        let mut index = Port::<u8>::new(CMOS_INDEX_PORT);
        let mut data = Port::<u8>::new(CMOS_DATA_PORT);
        index.write(reg);
        data.read()
    }
}

/// True while the RTC is mid-update and its time registers may be inconsistent.
fn update_in_progress() -> bool {
    read_register(REG_STATUS_A) & 0x80 != 0
}

/// Read the six time/date registers as raw bytes (still BCD/12h-encoded).
fn read_raw() -> DateTime {
    DateTime {
        second: read_register(REG_SECONDS),
        minute: read_register(REG_MINUTES),
        hour: read_register(REG_HOURS),
        day: read_register(REG_DAY),
        month: read_register(REG_MONTH),
        year: read_register(REG_YEAR) as u16,
    }
}

/// Convert a Binary-Coded-Decimal byte to its plain binary value.
/// e.g. 0x59 (BCD) -> 59.
fn bcd_to_binary(value: u8) -> u8 {
    (value & 0x0F) + ((value >> 4) * 10)
}

/// Read the current time from the RTC, fully decoded.
///
/// Uses the classic "read twice until two consecutive reads agree, around the
/// update-in-progress flag" technique so we never return a value captured
/// mid-tick.
pub fn read() -> DateTime {
    while update_in_progress() {}
    let mut last = read_raw();

    loop {
        while update_in_progress() {}
        let current = read_raw();
        if current == last {
            break;
        }
        last = current;
    }

    let status_b = read_register(REG_STATUS_B);
    let is_binary = status_b & 0x04 != 0; // bit 2 set => already binary, not BCD
    let is_24_hour = status_b & 0x02 != 0; // bit 1 set => 24-hour format

    // The 12/24-hour PM flag lives in bit 7 of the *raw* hour byte; capture it
    // before we strip/convert, because BCD decoding would mangle that bit.
    let hour_pm_flag = last.hour & 0x80 != 0;

    let mut dt = last;

    if !is_binary {
        dt.second = bcd_to_binary(dt.second);
        dt.minute = bcd_to_binary(dt.minute);
        dt.day = bcd_to_binary(dt.day);
        dt.month = bcd_to_binary(dt.month);
        dt.year = bcd_to_binary(dt.year as u8) as u16;
        // Decode hours from BCD ignoring the PM flag bit.
        dt.hour = bcd_to_binary(dt.hour & 0x7F);
    } else {
        dt.hour &= 0x7F;
    }

    // Convert 12-hour to 24-hour. 12 AM -> 0, 12 PM -> 12, PM -> +12.
    if !is_24_hour {
        dt.hour %= 12; // 12 -> 0
        if hour_pm_flag {
            dt.hour += 12;
        }
    }

    // The year register only stores two digits. The RTC has no reliable
    // century register across all hardware, so assume the 2000s — fine for a
    // hobby clock and matches what QEMU presents.
    dt.year += 2000;

    dt
}

/// Day of week via Zeller's congruence. 0 = Sunday .. 6 = Saturday.
pub fn day_of_week(dt: &DateTime) -> u8 {
    let (mut y, m) = (dt.year as i32, dt.month as i32);
    // Zeller treats Jan/Feb as months 13/14 of the previous year.
    let mm = if m < 3 { m + 12 } else { m };
    if m < 3 {
        y -= 1;
    }
    let k = y % 100; // year within century
    let j = y / 100; // century
    let q = dt.day as i32;
    // Zeller's formula yields 0 = Saturday; rotate so 0 = Sunday.
    let h = (q + (13 * (mm + 1)) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    ((h + 6) % 7) as u8
}

#[allow(dead_code)] // abbreviated names kept as reference; UI uses the full ones
pub fn weekday_name(index: u8) -> &'static str {
    const NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    NAMES[(index % 7) as usize]
}

#[allow(dead_code)]
pub fn month_name(month: u8) -> &'static str {
    const NAMES: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    NAMES[((month.saturating_sub(1)) % 12) as usize]
}

pub fn weekday_full(index: u8) -> &'static str {
    const NAMES: [&str; 7] = [
        "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday",
    ];
    NAMES[(index % 7) as usize]
}

pub fn month_full(month: u8) -> &'static str {
    const NAMES: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September",
        "October", "November", "December",
    ];
    NAMES[((month.saturating_sub(1)) % 12) as usize]
}
