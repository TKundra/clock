#![no_std] // no operating system underneath us — no libstd.
#![no_main] // we provide our own entry point, not the C runtime's `main`.

mod vga_buffer;
mod serial;
mod ui;

use core::panic::PanicInfo;

/// The bootloader finishes the switch to 64-bit long mode and then jumps here.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // Send a boot message to COM1.
    //
    // Useful if VGA is broken.
    serial_println!(
        "clock kernel — live bare-metal RTC clock"
    );

    // Draw static parts of the UI:
    //
    // - border
    // - title
    // - background
    //
    // These never change.
    {
        // Acquire exclusive access to VGA memory.
        let mut writer = vga_buffer::WRITER.lock();
        ui::draw_frame(&mut writer);
    } // lock released here

    loop {}
}

/// Panic handler.
///
/// Every panic eventually reaches here.
///
/// Example:
///
///     panic!("something failed");
///
/// Since there is no operating system:
///
/// - no stderr
/// - no crash dialog
/// - no process to terminate
///
/// we print the error and stop the CPU.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {

    println!("KERNEL PANIC: {}", info);

    loop {
        // HLT:
        //
        // Stop executing instructions until the next
        // interrupt arrives.
        //
        // This avoids wasting 100% CPU in:
        //
        //     loop {}
        x86_64::instructions::hlt();
    }
}
