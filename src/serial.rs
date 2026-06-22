//! Serial output driver for COM1 (16550 UART).
//! Before a kernel has:
//!
//! - graphics drivers
//! - a window system
//! - USB support
//! - keyboard input
//!
//! the serial port is often the only reliable debugging output.
//!
//! QEMU can redirect COM1 directly to the host terminal:
//!     qemu-system-x86_64 -serial stdio
//!
//! This means:
//!     println!("hello");
//!
//! can appear directly in your Linux terminal even if VGA output is broken.
//!
//! Most operating systems keep serial logging enabled because it still works
//! during crashes, panics, and early boot failures.

use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;

lazy_static! {
    /// Global COM1 serial port.
    ///
    /// COM1 is historically mapped to I/O ports:
    ///     0x3F8 - 0x3FF
    ///
    /// These addresses belong to the UART hardware.

    /// The UART (Universal Asynchronous Receiver/Transmitter)
    /// converts bytes into serial communication signals.
    pub static ref SERIAL1: Mutex<SerialPort> = {
        // Standard x86 COM1 base port.
        //
        // Other traditional ports:
        // COM1 -> 0x3F8
        // COM2 -> 0x2F8
        // COM3 -> 0x3E8
        // COM4 -> 0x2E8

        // SAFETY:
        // Accessing I/O ports is inherently unsafe because the CPU
        // cannot verify that hardware exists there.

        // On x86 systems 0x3F8 is the conventional COM1 address.
        let mut serial_port = unsafe { SerialPort::new(0x3F8) };

        // Initialize the UART. This configures:
        // - baud rate
        // - data bits
        // - parity
        // - stop bits

        // so that communication can begin.
        serial_port.init();

        // A spinlock is sufficient because the kernel does not yet
        // have threads or a scheduler.
        Mutex::new(serial_port)
    };
}

/// Internal printing implementation.
/// Rust formatting:
///     serial_println!("x = {}", x)
///
/// eventually becomes:
///     write_fmt(...)
///
/// which writes the formatted text to the UART.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    SERIAL1.lock().write_fmt(args).expect("printing to serial failed");
}

/// Print only to the serial port.
/// Unlike `println!`, this does NOT write to VGA.
///
/// Useful for:
/// - debugging
/// - crash logs
/// - headless servers
/// - QEMU console output
///
/// Example:
///     serial_println!("interrupt fired");
///
/// Output
///     host terminal
///
/// but the VGA screen remains unchanged.
#[macro_export]
macro_rules! serial_println {
    () => (
        $crate::serial::_print(
            format_args!("\n")
        )
    );

    ($($arg:tt)*) => (
        $crate::serial::_print(
            format_args!(
                "{}\n",
                format_args!($($arg)*)
            )
        )
    );
}