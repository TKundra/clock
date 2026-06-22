# Chapter 05 — The CMOS / RTC Driver

This is the heart of the project. We'll read the current wall-clock time
straight from the **Real-Time Clock** chip, with no operating system in between.

By the end you'll understand I/O ports, BCD encoding, the update-in-progress
race, and 12/24-hour decoding — and have a `rtc::read()` that returns a clean
`DateTime`.

## Background: what is the CMOS RTC?

On the original IBM PC, a Motorola **MC146818** chip kept the date and time
running off a small battery, so the machine knew the time at power-on. Its
descendants live in every PC chipset today. The same chip also holds a chunk of
battery-backed "CMOS" RAM (where the BIOS stored settings) — which is why people
say "the CMOS clock".

We can't read it like normal memory. It lives in the **I/O port** address space,
reached with the x86 `in`/`out` instructions, through two ports:

```
port 0x70  — INDEX port: write the register number you want to access
port 0x71  — DATA  port: then read/write that register's value
```

So every access is two steps: select a register on `0x70`, then read/write on
`0x71`.

### The registers we care about

| Index | Meaning |
|-------|---------|
| `0x00` | Seconds |
| `0x02` | Minutes |
| `0x04` | Hours |
| `0x07` | Day of month |
| `0x08` | Month |
| `0x09` | Year (two digits!) |
| `0x0A` | Status Register A (bit 7 = *update in progress*) |
| `0x0B` | Status Register B (bit 1 = 24-hour, bit 2 = binary vs BCD) |

## Step 1: dependency for port I/O

We use the `x86_64` crate's `Port` type so we don't have to write inline
assembly for `in`/`out`:

```toml
# Cargo.toml  [dependencies]
x86_64 = "0.14"
```

## Step 2: ports, registers, and a result type

```rust
// src/rtc.rs
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

/// A point in time as reported by the RTC, decoded to plain binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}
```

`#[derive(PartialEq, Eq)]` is important — we'll compare two `DateTime`s for
equality to detect the update race.

## Step 3: read a single register

```rust
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
        index.write(reg);   // select the register
        data.read()         // read its value
    }
}
```

Two subtleties:

- **It's `unsafe`** because port I/O can have arbitrary hardware side effects;
  the compiler can't reason about it. We assert these specific ports are safe.
- **The NMI bit:** bit 7 of port `0x70` controls whether Non-Maskable Interrupts
  are disabled. By writing the bare register index (bit 7 = 0) we leave NMIs
  enabled. (Some kernels deliberately preserve/clear this bit; for us, plain
  writes are fine.)

## Step 4: the update-in-progress race

Roughly once per second the RTC updates its own registers. If you read the six
time bytes *while* it's updating, you can catch an inconsistent mix (e.g.
seconds rolled over to 00 but minutes haven't ticked yet → you read 11:59:00
instead of 12:00:00).

Status Register A bit 7 is the **"update in progress" (UIP)** flag:

```rust
/// True while the RTC is mid-update and its time registers may be inconsistent.
fn update_in_progress() -> bool {
    read_register(REG_STATUS_A) & 0x80 != 0
}
```

The robust, widely-used technique (from the OSDev wiki) is: **read all values,
then read them again, and only accept the result when two consecutive reads
agree** — each read taken while UIP is clear. If they differ, an update happened
in between, so try again.

First, a helper to grab all six raw bytes:

```rust
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
```

## Step 5: BCD vs binary

By default the RTC stores values in **Binary-Coded Decimal**: each *nibble*
(4 bits) holds one decimal digit. So the decimal number 59 is stored as the byte
`0x59` (`0101_1001` = nibble 5, nibble 9) — *not* as binary `0x3B`.

Conversion is "high nibble × 10 + low nibble":

```rust
/// Convert a Binary-Coded-Decimal byte to its plain binary value.
/// e.g. 0x59 (BCD) -> 59.
fn bcd_to_binary(value: u8) -> u8 {
    (value & 0x0F) + ((value >> 4) * 10)
}
```

Whether the chip is in BCD or binary mode is told by **bit 2 of Status Register
B**. Whether hours are 12- or 24-hour is **bit 1**.

## Step 6: put it together — `read()`

```rust
/// Read the current time from the RTC, fully decoded.
pub fn read() -> DateTime {
    while update_in_progress() {}
    let mut last = read_raw();

    loop {
        while update_in_progress() {}
        let current = read_raw();
        if current == last {
            break;          // two consecutive reads agree → stable value
        }
        last = current;
    }

    let status_b = read_register(REG_STATUS_B);
    let is_binary = status_b & 0x04 != 0; // bit 2 set => already binary, not BCD
    let is_24_hour = status_b & 0x02 != 0; // bit 1 set => 24-hour format

    // The 12/24-hour PM flag lives in bit 7 of the RAW hour byte; capture it
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
        dt.hour %= 12;          // 12 -> 0
        if hour_pm_flag {
            dt.hour += 12;
        }
    }

    // The year register only stores two digits. There's no universally reliable
    // century register, so assume the 2000s — fine for a hobby clock and what
    // QEMU presents.
    dt.year += 2000;

    dt
}
```

The tricky bits, called out:

1. **Capture the PM flag *before* decoding.** In 12-hour mode, bit 7 of the hour
   byte means "PM". If we BCD-decoded first, that bit would corrupt the math, so
   we read `hour_pm_flag` from the raw byte and mask it off (`& 0x7F`) before
   decoding.
2. **12-hour → 24-hour.** `hour % 12` maps 12→0; then add 12 if PM. So 12 AM→0,
   1 PM→13, 12 PM→12. ✔
3. **Two-digit year.** Register `0x09` gives `26`, not `2026`. We add `2000`.

> Most real hardware (and QEMU) defaults to **BCD + 24-hour**, so in practice
> the BCD branch runs and the 12-hour branch doesn't — but handling both makes
> the driver correct on any configuration.

## Step 7: the weekday (Zeller's congruence)

The RTC *can* store a day-of-week register, but it's often not maintained.
Instead we compute it from the date with **Zeller's congruence**, a classic
closed-form formula:

```rust
/// Day of week via Zeller's congruence. 0 = Sunday .. 6 = Saturday.
pub fn day_of_week(dt: &DateTime) -> u8 {
    let (mut y, m) = (dt.year as i32, dt.month as i32);
    // Zeller treats Jan/Feb as months 13/14 of the PREVIOUS year.
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
```

The quirks: January and February count as months 13 and 14 of the *previous*
year (so leap-day handling works out), and raw Zeller numbers Saturday as 0, so
we rotate by 6 to make Sunday 0.

## Step 8: name lookup tables

```rust
pub fn weekday_name(index: u8) -> &'static str {
    const NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    NAMES[(index % 7) as usize]
}

pub fn month_name(month: u8) -> &'static str {
    const NAMES: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    NAMES[((month.saturating_sub(1)) % 12) as usize]
}
```

The `% 7` / `saturating_sub(1) % 12` guard against out-of-range indices so a
glitchy read can never panic with an array-bounds error.

## Step 9: register the module

```rust
// src/main.rs
mod rtc;       // <-- add
mod serial;
mod vga_buffer;
```

## Checkpoint

`rtc::read()` now returns a fully-decoded `DateTime`, and you can compute the
weekday and names. Everything talks to the hardware directly — no syscalls, no
libraries hiding the details. Next we expose it as a `date` command and print
it.

---

Prev: [Chapter 04 — Serial port](04-serial-port.md) ·
Next: [Chapter 06 — The `date` command →](06-date-command.md)
