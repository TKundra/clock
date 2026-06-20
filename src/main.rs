#![no_std] // no operating system underneath us — no libstd.
#![no_main] // we provide our own entry point, not the C runtime's `main`.

mod rtc;
mod serial;
mod ui;
mod vga_buffer;

use core::panic::PanicInfo;

/// The bootloader finishes the switch to 64-bit long mode and then jumps here.
/// It never returns (`!`) — there is nowhere to return to.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    serial_println!("clock kernel — live bare-metal RTC clock");

    // Draw the static UI once, then run the live clock loop forever.
    {
        let mut writer = vga_buffer::WRITER.lock();
        ui::draw_frame(&mut writer);
    }

    run_clock()
}

/// The realtime clock: poll the RTC and redraw only when the second changes.
///
/// We have no timer interrupt yet (see learning/08), so this is a polling loop.
/// `rtc::read()` already waits for the chip's update to finish, so each read is
/// a stable value; we compare against the last one and only repaint on a tick —
/// which keeps the screen flicker-free and the serial log to one line/second.
fn run_clock() -> ! {
    let mut last: Option<rtc::DateTime> = None;
    let mut blink = false;

    loop {
        let now = rtc::read();

        if Some(now) != last {
            blink = !blink;

            {
                let mut writer = vga_buffer::WRITER.lock();
                ui::draw_dynamic(&mut writer, &now, blink);
            }

            // Mirror each tick to the serial console (headless log + proof the
            // clock is advancing in real time).
            serial_println!(
                "{}-{:02}-{:02}T{:02}:{:02}:{:02}",
                now.year, now.month, now.day, now.hour, now.minute, now.second
            );

            last = Some(now);
        }

        // Tiny pause so we don't hammer the CMOS ports between ticks.
        for _ in 0..50_000 {
            core::hint::spin_loop();
        }
    }
}

/// Called by the compiler on `panic!`. With no OS we just print and halt.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("KERNEL PANIC: {}", info);
    loop {
        x86_64::instructions::hlt();
    }
}
