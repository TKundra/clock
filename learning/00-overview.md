# Chapter 00 — Overview & Mental Model

Before writing code, let's build a mental model of what we're doing and why each
piece exists.

## What "bare metal" means here

Normally a Rust program runs *on top of* an operating system. The OS gives you:
threads, a heap, files, `println!` to a terminal, and a `main` the C runtime
calls for you.

We have **none of that**. Our code runs directly on the CPU, right after the
bootloader hands over control. That has three big consequences:

1. **No `std`.** We use `#![no_std]` and only the `core` library (no heap, no
   `Vec`, no `String`, no `println!`).
2. **No `main`.** We define our own entry point, `_start`, and tell the linker
   about it.
3. **No drivers.** Want to put a character on screen? Write to video memory
   yourself. Want the time? Talk to the clock chip yourself.

That last point is the whole project: we read the time by talking to hardware.

## The boot flow

Here's the chain of events from power-on to our code running:

```
┌──────────┐   ┌────────────┐   ┌─────────────────────┐   ┌──────────────┐
│  BIOS /  │ → │ bootloader │ → │ switch to 64-bit     │ → │ our _start() │
│  SeaBIOS │   │  (stage 1) │   │ long mode, set up    │   │ runs in      │
│          │   │            │   │ paging, load kernel  │   │ long mode    │
└──────────┘   └────────────┘   └─────────────────────┘   └──────────────┘
```

- **BIOS** finds a bootable disk and loads its first sector.
- The **`bootloader` crate** (we depend on it; we don't write it) does the
  tedious 16-bit → 32-bit → 64-bit mode transition, sets up page tables, loads
  our kernel ELF into memory, and finally jumps to our `_start`.
- **Our kernel** runs in 64-bit long mode with a flat memory space. From here
  on, it's our code.

We use the `bootloader = "0.9"` crate plus the `bootimage` tool, which glues our
compiled kernel together with the bootloader into a single bootable disk image.

## The hardware we touch

Two pieces of hardware matter:

### 1. The VGA text buffer (output)

At physical address `0xb8000` there's a region of memory that *is* the screen.
In text mode it's an 80×25 grid; each cell is 2 bytes (one ASCII byte + one
color byte). Write a byte there and a character appears. No syscall, no driver —
it's just memory. (Chapter 03.)

### 2. The CMOS / RTC (the time)

The **Real-Time Clock** is a tiny battery-backed chip that keeps wall-clock time
even when the machine is off. You can't read it as memory; you talk to it
through **I/O ports** — a separate address space accessed with the `in`/`out`
CPU instructions:

- Port `0x70` — the *index* port: "which register do you want?"
- Port `0x71` — the *data* port: read/write that register's value.

The time lives in numbered registers (seconds at index `0x00`, minutes at
`0x02`, …). Reading it correctly means handling two quirks — values are stored
in **BCD** (binary-coded decimal) and the chip **updates once a second**, so you
must avoid reading mid-update. That's the meat of Chapter 05.

## The module map

By the end, the project looks like this:

```
clock/
├── Cargo.toml              # deps + bootimage QEMU args
├── rust-toolchain.toml     # pin nightly + components
├── x86_64-clock.json       # our custom bare-metal target
├── .cargo/config.toml      # build-std, default target, runner
└── src/
    ├── main.rs             # _start, panic handler, the realtime clock loop
    ├── vga_buffer.rs       # println! + positioned drawing to the screen
    ├── serial.rs           # COM1 output for debugging
    ├── ui.rs               # the on-screen clock panel (box, big time, heartbeat)
    └── rtc.rs              # THE CMOS/RTC driver — the point of it all
```

## The one bug you'll hit (spoiler)

When we first boot, the bootloader panics: *"failed to map segment at
Page(0x0)"*. That's because the modern built-in bare-metal target produces a
**position-independent executable** with a chunk at virtual address `0x0`, which
`bootloader` 0.9 can't handle. The fix is a custom target that produces a plain,
non-relocatable executable. We cover the symptom, diagnosis, and fix in
[Chapter 08](08-build-boot-verify.md) — but Chapter 01 sets up the fixed target
from the start so you don't trip on it.

---

Next: [Chapter 01 — Toolchain & target →](01-toolchain-and-target.md)
