# Building a Bare-Metal RTC Clock in Rust

A chapter-by-chapter guide to building a tiny `no_std` x86-64 kernel that boots
under QEMU and prints the **real wall-clock time** by reading the CMOS
Real-Time Clock directly over I/O ports.

This is a "learn by rebuilding" guide. Each chapter adds one piece, explains the
*why*, and gives you the exact code to type. By the end you'll have re-created
the whole project and understand every line.

## What you'll build

A bootable kernel that prints:

```
╔════════════════════════════════════════════════╗
║             BARE-METAL  RTC  CLOCK               ║
╠════════════════════════════════════════════════╣
║              Friday, 19 June 2026                ║
║                  16 : 00 : 28                    ║
║              2026-06-19T16:00:28                 ║
║      live  reading CMOS RTC @ 0x70/0x71 •        ║
╚════════════════════════════════════════════════╝
```

…a live panel where the time **ticks every second**, read straight from the RTC
hardware — not from any operating system.

## Prerequisites

- Comfort with Rust basics (structs, traits, modules, `unsafe`).
- A Linux host with `rustup`, `qemu-system-x86_64` installed.
- Curiosity about how `date` works *below* the operating system.

You do **not** need prior kernel experience — but if you've done the
[blog_os](https://os.phil-opp.com/) "minimal kernel" and "VGA text mode" posts,
chapters 1–4 will feel familiar and you can skim them.

## Chapters

| # | Chapter | What you learn |
|---|---------|----------------|
| 00 | [Overview & mental model](learning/00-overview.md) | The boot flow, what "bare metal" means, the moving parts |
| 01 | [Toolchain & target](learning/01-toolchain-and-target.md) | nightly, `build-std`, the custom `no_std` target JSON |
| 02 | [The freestanding binary](learning/02-freestanding-binary.md) | `#![no_std]`, `#![no_main]`, `_start`, the panic handler |
| 03 | [VGA text buffer](learning/03-vga-text-buffer.md) | Writing to screen memory at `0xb8000`, `println!` |
| 04 | [Serial port output](learning/04-serial-port.md) | A COM1 console for headless debugging |
| 05 | [The CMOS/RTC driver](learning/05-cmos-rtc-driver.md) | **The heart:** port I/O, BCD, update races, time decode |
| 06 | [The `date` command](learning/06-date-command.md) | Wiring the driver into a one-shot command + weekday math |
| 07 | [A realtime clock with a UI](learning/07-realtime-clock-ui.md) | Positioned drawing, a boxed UI, the poll-redraw-on-tick loop |
| 08 | [Build, boot & verify](learning/08-build-boot-verify.md) | `bootimage`, QEMU, the PIE bug, verifying the ticking |
| 09 | [Where to go next](learning/09-next-steps.md) | Interrupt-driven clock, a real keyboard shell |

## How to use this guide

1. Read chapter 00 for the big picture.
2. From chapter 01 onward, create each file as you go and build after each step.
3. If you get stuck, the finished source in `src/` is the reference.

Start with [Chapter 00 →](learning/00-overview.md)
