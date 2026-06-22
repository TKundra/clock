# Chapter 04 — Serial Port Output

Goal: add a serial console so the kernel can print text that QEMU pipes straight
to your terminal — even with no display window. This is the single most useful
debugging tool in kernel development.

## Why bother, when we already have VGA?

VGA output is *on the emulated screen*. To read it from a script (or when
something crashes before the display initializes) you'd have to screenshot the
QEMU window or poke at video memory through the monitor. Painful.

The **serial port** (COM1) is a far older, simpler device. QEMU can connect
COM1 directly to your host terminal with one flag:

```bash
qemu-system-x86_64 … -serial stdio    # COM1 bytes appear on your stdout
```

So: mirror every `println!` to serial, and you can `grep` your kernel's output,
capture it to a file, and debug headlessly. (We'll see in Chapter 08 how this
saved us when VGA was the *only* thing working.)

## Step 1: add the UART driver dependency

The serial port is driven by a "16550 UART" chip. Rather than poke its registers
by hand, we use a small crate:

```toml
# Cargo.toml  [dependencies]
uart_16550 = "0.2.0"
```

(You *could* write this yourself — it's ~5 port writes to configure baud rate,
line control, and FIFO — but it's not the point of this project, so we lean on
the crate.)

## Step 2: the serial module

```rust
// src/serial.rs
use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;

lazy_static! {
    pub static ref SERIAL1: Mutex<SerialPort> = {
        // 0x3F8 is the standard I/O port base for COM1.
        // SAFETY: 0x3F8 is the conventional COM1 base port on x86.
        let mut serial_port = unsafe { SerialPort::new(0x3F8) };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    SERIAL1
        .lock()
        .write_fmt(args)
        .expect("printing to serial failed");
}
```

`0x3F8` is the well-known I/O port base for the first serial port (COM1).
`SerialPort::new` + `init()` configures it; after that `write_fmt` works because
`SerialPort` implements `core::fmt::Write` for us.

## Step 3: mirror VGA output to serial

We don't want to call two print functions everywhere. Instead, make the VGA
`_print` also forward to serial. Edit `src/vga_buffer.rs`:

```rust
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    WRITER.lock().write_fmt(args).unwrap();
    // Mirror everything to the serial console so headless QEMU shows it too.
    crate::serial::_print(args);
}
```

Now every `println!` lands on **both** the VGA screen and the serial line.

> **Lock-ordering note:** `_print` takes the VGA lock, then (inside
> `serial::_print`) the serial lock — always in that order, and they're
> different locks, so there's no deadlock. If you ever add interrupt handlers
> that also print, you'll need to be more careful (disable interrupts while
> holding these locks), but for our straight-line kernel it's fine.

## Step 4: register the module

```rust
// src/main.rs
mod rtc;       // (coming in Chapter 05)
mod serial;    // <-- add
mod vga_buffer;
```

## A serial-only macro

Sometimes you want output that goes *only* to serial — a log that doesn't
disturb the on-screen UI. The realtime clock in Chapter 07 uses exactly this to
print one line per tick (so `-serial stdio` gives a ticking log while the screen
shows the drawn panel). Add it now:

```rust
// src/serial.rs
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial::_print(format_args!("\n")));
    ($($arg:tt)*) => ($crate::serial::_print(format_args!("{}\n", format_args!($($arg)*))));
}
```

`serial_println!` routes straight to `serial::_print` (serial only), whereas the
`println!` from Chapter 03 goes to *both* screen and serial.

## Checkpoint

Output now goes to both screen and serial. We have everything we need to *show*
results. Time for the actual subject of the project: reading the clock.

---

Prev: [Chapter 03 — VGA text buffer](03-vga-text-buffer.md) ·
Next: [Chapter 05 — The CMOS/RTC driver →](05-cmos-rtc-driver.md)
