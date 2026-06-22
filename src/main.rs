#![no_std]
// No standard library.
//
// The Rust standard library assumes an operating system exists.
//
// Examples:
//
//     std::thread
//     std::fs
//     std::net
//     println!
//
// None of these exist in our kernel yet.
//
// We only use `core`, which provides:
// - Option
// - Result
// - formatting traits
// - iterators
// - basic language features

#![no_main]
// Disable Rust's normal entry point.
//
// A normal Rust program:
//
//     fn main()
//
// becomes:
//
//     C runtime
//         ↓
//     main()
//
// But kernels do not have:
// - libc
// - crt0
// - an operating system
//
// The bootloader jumps directly into our code.

mod rtc;
mod serial;
mod ui;
mod vga_buffer;

use core::panic::PanicInfo;

/// Called by Rust whenever:
///
///     panic!("oops")
///
/// occurs.
///
/// Normal applications unwind the stack or terminate the process.
///
/// A kernel has no process to terminate, so we must decide what to do.
use core::panic::PanicInfo;

/// Kernel entry point.
///
/// Boot sequence:
///
///     BIOS/UEFI
///          ↓
///     Bootloader
///          ↓
///     64-bit long mode
///          ↓
///        _start()
///
/// The bootloader has already:
///
/// - loaded the kernel
/// - enabled protected mode
/// - enabled paging
/// - entered 64-bit mode
///
/// Therefore `_start()` is the first Rust code that executes.
#[no_mangle]
// Prevent Rust from renaming the symbol.
//
// Without this the compiler might generate:
//
//     _ZN6kernel7_start...
//
// The bootloader specifically looks for "_start".
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
        let mut writer =
            vga_buffer::WRITER.lock();

        ui::draw_frame(&mut writer);
    } // lock released here

    // Start the realtime clock loop.
    run_clock()
}

/// Main kernel loop.
///
/// The function returns `!` because kernels never exit.
///
/// We currently do not have:
///
/// - timer interrupts
/// - task scheduling
/// - event loops
///
/// so we repeatedly poll the RTC.
fn run_clock() -> ! {

    // Previous timestamp.
    //
    // Initially there is no previous value.
    let mut last: Option<rtc::DateTime> = None;

    // Controls the blinking indicator.
    let mut blink = false;

    loop {
        // Read current RTC time.
        let now = rtc::read();

        // Only redraw when the second changes.
        //
        // This avoids:
        //
        // - unnecessary VGA writes
        // - flickering
        // - serial spam
        if Some(now) != last {

            blink = !blink;

            {
                let mut writer =
                    vga_buffer::WRITER.lock();

                ui::draw_dynamic(
                    &mut writer,
                    &now,
                    blink,
                );
            }

            // Print the timestamp to COM1.
            //
            // Benefits:
            //
            // - debugging
            // - headless operation
            // - proof the RTC is advancing
            serial_println!(
                "{}-{:02}-{:02}T{:02}:{:02}:{:02}",
                now.year,
                now.month,
                now.day,
                now.hour,
                now.minute,
                now.second
            );

            last = Some(now);
        }

        // Small delay.
        //
        // Without this loop we would read the CMOS
        // thousands or millions of times per second.
        //
        // spin_loop() tells the CPU:
        //
        //     "I am intentionally busy-waiting."
        //
        // Some processors can optimize power usage
        // while spinning.
        for _ in 0..50_000 {
            core::hint::spin_loop();
        }
    }
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