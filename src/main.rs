#![no_std] // no operating system underneath us — no libstd.
#![no_main] // we provide our own entry point, not the C runtime's `main`.

mod vga_buffer;
mod serial;
mod ui;
mod rtc;

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
