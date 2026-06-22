//! CMOS Real-Time Clock (RTC) driver.
//
//! The RTC is a tiny battery-powered clock built into the motherboard.
//! Even when the computer is completely powered off, the RTC continues
//! running using the CMOS battery.
//
//! Historically this chip was the Motorola MC146818.
//! Modern PCs emulate the same interface for compatibility.
//
//! The RTC is accessed through two I/O ports:
//
//!     0x70 = register selector
//!     0x71 = register data
//
//! Registers are:
//! These are tiny storage locations inside the CPU itself.
// They are extremely fast and hold data the processor is currently working on.
//
//! Reading a value requires two steps:
//
//!     write register number to 0x70
//!     read result from 0x71
//
//! Example:
//
//!     write 0x04 to 0x70   (hour register)
//!     read  from 0x71      (current hour)
//
//! Unfortunately RTC access is not as simple as reading six registers:
//
//! 1. The chip updates itself once per second.
//!    Reading during an update may produce inconsistent values.
//
//!       12:59:59
//!            ↓
//!       13:00:00
//!
//!    If we read during this transition we might see:
//
//!       hour   = 12
//!       minute = 00
//!       second = 00
//
//!    which is invalid.
//
//! 2. Many RTCs store numbers in BCD instead of binary.
//
//!       59 decimal
//!           ↓
//!       0x59
//!
//!    which is NOT hexadecimal 0x59 (= 89 decimal).

use x86_64::instructions::port::Port;

/// Standard CMOS index port.
///
/// Writing here selects which register we want to access.
const CMOS_INDEX_PORT: u16 = 0x70;

/// CMOS data port.
///
/// After selecting a register through 0x70,
/// reading from 0x71 returns its value.
const CMOS_DATA_PORT: u16 = 0x71;

// ======================================================
// RTC register numbers
// ======================================================
//
// 0x00 -> seconds
// 0x02 -> minutes
// 0x04 -> hours
//
// These numbers are defined by the original RTC hardware
// and remain standardized on modern PCs.

const REG_SECONDS: u8 = 0x00;
const REG_MINUTES: u8 = 0x02;
const REG_HOURS: u8 = 0x04;
const REG_DAY: u8 = 0x07;
const REG_MONTH: u8 = 0x08;
const REG_YEAR: u8 = 0x09;

/// Status register A.
///
/// Bit 7:
///
///     1 = update in progress
///     0 = safe to read
const REG_STATUS_A: u8 = 0x0A;

/// Status register B.
///
/// Bit 2:
///
///     1 = binary mode
///     0 = BCD mode
///
/// Bit 1:
///
///     1 = 24-hour clock
///     0 = 12-hour clock
const REG_STATUS_B: u8 = 0x0B;

/// Fully decoded calendar date and time.
///
/// All values are already converted to:
///
/// - binary numbers
/// - 24-hour format
/// - full year (2026 instead of 26)
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
/// Hardware access sequence:
///
///     write register number
///     read register value
///
/// The upper bit of port 0x70 controls the NMI
/// (Non-Maskable Interrupt) disable line.
///
/// Since we write only the register number itself,
/// NMIs remain enabled.
fn read_register(reg: u8) -> u8 {
    // SAFETY:
    //
    // Ports 0x70 and 0x71 are reserved by the PC architecture
    // for CMOS access.
    unsafe {
        let mut index = Port::<u8>::new(CMOS_INDEX_PORT);
        let mut data = Port::<u8>::new(CMOS_DATA_PORT);

        // Select register.
        index.write(reg);

        // Read register value.
        data.read()
    }
}

/// True while the RTC is updating its internal registers.
///
/// During this period the clock may contain:
///
///     seconds = old value
///     minutes = new value
///
/// which would produce invalid timestamps.
fn update_in_progress() -> bool {
    read_register(REG_STATUS_A) & 0x80 != 0
}

/// Read raw RTC values.
///
/// The returned values may still contain:
///
/// - BCD encoding
/// - 12-hour encoding
/// - PM flag in bit 7
///
/// No decoding happens here.
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

/// Convert Binary-Coded Decimal to binary.
///
/// BCD stores each decimal digit separately:
///
///     0x59
///
/// becomes:
///
///     high nibble = 5
///     low nibble  = 9
///
/// Result:
///
///     5 * 10 + 9 = 59
///
/// Example:
///
///     0x42 -> 42
///     0x18 -> 18
fn bcd_to_binary(value: u8) -> u8 {
    (value & 0x0F) + ((value >> 4) * 10)
}

/// Read the current RTC time.
///
/// This uses the classic RTC stabilization algorithm:
///
///     wait until not updating
///     read values
///     wait again
///     read again
///
/// If both reads match:
///
///     safe
///
/// Otherwise:
///
///     try again
///
/// This prevents returning partially updated timestamps.
pub fn read() -> DateTime {
    // Wait for update to finish.
    while update_in_progress() {}

    let mut last = read_raw();

    loop {
        while update_in_progress() {}

        let current = read_raw();

        // Two identical reads means the values are stable.
        if current == last {
            break;
        }

        last = current;
    }

    let status_b = read_register(REG_STATUS_B);

    // Bit 2:
    //
    //     1 = already binary
    //     0 = BCD
    let is_binary = status_b & 0x04 != 0;

    // Bit 1:
    //
    //     1 = 24-hour mode
    //     0 = 12-hour mode
    let is_24_hour = status_b & 0x02 != 0;

    // In 12-hour mode bit 7 stores the PM flag:
    //
    //     0 = AM
    //     1 = PM
    //
    // We must save it before decoding BCD.
    let hour_pm_flag = last.hour & 0x80 != 0;

    let mut dt = last;

    if !is_binary {
        dt.second = bcd_to_binary(dt.second);
        dt.minute = bcd_to_binary(dt.minute);
        dt.day = bcd_to_binary(dt.day);
        dt.month = bcd_to_binary(dt.month);
        dt.year = bcd_to_binary(dt.year as u8) as u16;

        // Remove PM flag before decoding.
        dt.hour = bcd_to_binary(dt.hour & 0x7F);
    } else {
        // Strip PM flag bit.
        dt.hour &= 0x7F;
    }

    // Convert:
    //
    //     12 AM -> 0
    //     1 PM  -> 13
    //     11 PM -> 23
    if !is_24_hour {
        dt.hour %= 12;

        if hour_pm_flag {
            dt.hour += 12;
        }
    }

    // RTC only stores two digits:
    //
    //     26
    //
    // becomes:
    //
    //     2026
    //
    // Many machines have a century register,
    // but its location is not standardized.
    dt.year += 2000;

    dt
}

/// Calculate weekday using Zeller's Congruence.
///
/// Result:
///
///     0 = Sunday
///     1 = Monday
///     ...
///     6 = Saturday
///
/// The algorithm treats:
///
///     January  -> month 13
///     February -> month 14
///
/// of the previous year.
pub fn day_of_week(dt: &DateTime) -> u8 {
    let (mut y, m) = (dt.year as i32, dt.month as i32);

    let mm = if m < 3 {
        m + 12
    } else {
        m
    };

    if m < 3 {
        y -= 1;
    }

    // Year within century.
    let k = y % 100;

    // Century.
    let j = y / 100;

    let q = dt.day as i32;

    let h =
        (q
            + (13 * (mm + 1)) / 5
            + k
            + k / 4
            + j / 4
            + 5 * j) % 7;

    // Zeller:
    //
    //     0 = Saturday
    //
    // We rotate:
    //
    //     0 = Sunday
    ((h + 6) % 7) as u8
}
