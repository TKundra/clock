//! Serial port (COM1 / 16550 UART) output.
//!
//! A serial console is the kernel-dev workhorse: QEMU can pipe COM1 straight to
//! the host terminal (`-serial stdio`), so we get text output even with no
//! display. Everything printed to the VGA screen is mirrored here too.

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

/// Print to the serial port ONLY (not the VGA screen). Useful for a headless
/// log that doesn't disturb the on-screen UI.
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial::_print(format_args!("\n")));
    ($($arg:tt)*) => ($crate::serial::_print(format_args!("{}\n", format_args!($($arg)*))));
}
