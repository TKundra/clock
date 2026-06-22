#![no_std] // no operating system underneath us — no libstd.
#![no_main] // we provide our own entry point, not the C runtime's `main`.

use core::panic::PanicInfo;

/// The bootloader finishes the switch to 64-bit long mode and then jumps here.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    loop {}
}

/// Called by the compiler on `panic!`. With no OS we just spin.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}