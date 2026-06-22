# Chapter 06 — The `date` Command

Goal: tie the RTC driver into a small command dispatcher, so `_start` runs a
`date` command that reads and prints the real time. This is the layer that makes
the kernel feel like it *does something*.

## A tiny command dispatcher

We have no keyboard yet (that's a Chapter 09 extension), so there's no shell to
type into. But we can still structure the code as if there were one: a
`run_command(name)` function that matches a command name and dispatches. This
makes it trivial to add `help`, `clear`, etc. later, and to hook up a keyboard
when you're ready.

## The full `src/main.rs`

```rust
#![no_std] // no operating system underneath us — no libstd.
#![no_main] // we provide our own entry point, not the C runtime's `main`.

mod rtc;
mod serial;
mod vga_buffer;

use core::panic::PanicInfo;

/// The bootloader finishes the switch to 64-bit long mode and then jumps here.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    vga_buffer::WRITER.lock().clear_screen();

    println!("clock kernel — bare-metal RTC reader");
    println!("type-free shell: running built-in commands\n");

    // Dispatch the one command we ship: `date`.
    run_command("date");

    println!("\n(idle — press the QEMU window's close button to exit)");

    // Halt the CPU between interrupts instead of spinning hot forever.
    loop {
        x86_64::instructions::hlt();
    }
}

/// Tiny command dispatcher. Easy to grow into a real keyboard-driven shell
/// later — for now `_start` calls it directly.
fn run_command(name: &str) {
    match name {
        "date" => cmd_date(),
        other => println!("unknown command: {}", other),
    }
}

/// The `date` command: read the real-time clock and print it.
fn cmd_date() {
    let now = rtc::read();
    let weekday = rtc::weekday_name(rtc::day_of_week(&now));

    // e.g. "Fri Jun 19 2026  14:03:07"
    println!(
        "{} {} {:02} {}  {:02}:{:02}:{:02}",
        weekday,
        rtc::month_name(now.month),
        now.day,
        now.year,
        now.hour,
        now.minute,
        now.second,
    );
    // ISO-8601 form too, since it's the unambiguous one.
    println!(
        "ISO-8601: {}-{:02}-{:02}T{:02}:{:02}:{:02}",
        now.year, now.month, now.day, now.hour, now.minute, now.second,
    );
}

/// Called by the compiler on `panic!`. With no OS we just print and halt.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("KERNEL PANIC: {}", info);
    loop {
        x86_64::instructions::hlt();
    }
}
```

## Things worth noticing

### `{:02}` zero-padding

`core::fmt` supports format specifiers just like std. `{:02}` means "at least 2
digits, pad with zeros" — so `7` prints as `07`. We get this for free because
our `Writer` implements `fmt::Write` (Chapter 03).

### `hlt` instead of a busy loop

The final loop uses `x86_64::instructions::hlt()` rather than an empty `loop {}`.
`hlt` halts the CPU until the next interrupt, so the emulated CPU idles at ~0%
instead of pinning a host core spinning. (We have no interrupts enabled yet, so
it effectively halts forever — exactly what we want for "done, idle".)

### The panic handler now prints

Because we built `println!` in Chapter 03 and it routes to both VGA and serial,
a panic message is now visible on screen *and* captured on the serial line —
invaluable when something goes wrong.

## Why two output formats?

- The human-friendly `Fri Jun 19 2026  15:49:21` is easy to read at a glance.
- The `ISO-8601: 2026-06-19T15:49:21` form is unambiguous (no locale guessing
  about day/month order) and easy to compare against the host's `date` output
  when verifying.

## A stepping stone, not the final form

This one-shot `date` is the simplest thing that proves the driver works: read
once, print, halt. It's worth building and booting first (Chapter 08) so you
see a real RTC read before adding more moving parts.

But a clock should **tick**. In Chapter 07 we evolve this: `_start` draws a UI
once and then enters a loop that re-reads the RTC and repaints in place every
second. The `run_command`/`cmd_date` code here gets replaced by that live loop —
but everything *below* it (the `rtc` driver, `println!`, the panic handler)
stays exactly as-is. So nothing you've written is wasted; we're swapping the top
layer for a livelier one.

## Checkpoint

The kernel is functionally complete as a *one-shot*: it boots, reads the RTC,
and prints the time two ways. We just can't *see* it yet — we've only ever run
`cargo build`. Next we make it tick.

---

Prev: [Chapter 05 — CMOS/RTC driver](05-cmos-rtc-driver.md) ·
Next: [Chapter 07 — A realtime clock with a UI →](07-realtime-clock-ui.md)
