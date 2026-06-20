# Chapter 02 — The Freestanding Binary

Goal: replace the default `main.rs` with a `no_std`, `no_main` binary that
compiles for our target and has a valid entry point and panic handler. It won't
print anything yet — that's Chapter 03 — but it will *build*.

## Why we can't keep `main`

A normal Rust binary's flow is:

```
C runtime (crt0) → calls `main` → Rust std setup → your code
```

We have no C runtime and no std. So we throw away both:

- `#![no_std]` — don't link the standard library.
- `#![no_main]` — don't use the normal entry point machinery.

…and we provide our own entry point that the bootloader will jump to.

## The minimal `src/main.rs`

```rust
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
```

Let's unpack the three magic pieces.

### `_start` — the entry point

```rust
#[no_mangle]
pub extern "C" fn _start() -> ! { … }
```

- **`_start`** is the conventional entry-point symbol name. The `bootloader`
  crate jumps to it after setting up long mode.
- **`#[no_mangle]`** keeps the symbol named *exactly* `_start` (Rust normally
  mangles names). The linker has to find it by that name.
- **`extern "C"`** gives it the C calling convention, which is what the
  bootloader expects.
- **`-> !`** ("never type") means it never returns. There's nowhere to return
  *to* — nothing called us in the usual sense. If you let `_start` fall off the
  end, you'd execute garbage. So it ends in an infinite loop (later, a
  `hlt`-loop).

### `#[panic_handler]` — what happens on `panic!`

In std, panicking unwinds the stack and prints to stderr. We have neither, so we
must supply the function the compiler calls on panic. It also returns `!`. For
now it just spins; in Chapter 03 we'll make it print the panic message first.

### Where did `eh_personality` go?

Older guides also need an `eh_personality` language item for stack unwinding.
We sidestep that entirely with `"panic-strategy": "abort"` in our target JSON
(Chapter 01) — no unwinding means no personality function needed.

## Build it

```bash
cargo build
```

This should now compile (it cross-compiles `core` the first time, so it's a bit
slow). You'll get a kernel ELF at `target/x86_64-clock/debug/clock`. It does
nothing but spin — but it's a real freestanding x86-64 binary.

You can sanity-check that it's the right *kind* of binary:

```bash
readelf -h target/x86_64-clock/debug/clock | grep -E 'Type|Entry'
# Type:  EXEC (Executable file)      <- not DYN/PIE, good (thanks to Chapter 01)
# Entry point address:  0x2089d0     <- in the 0x200000 range we linked at
```

If you see `Type: DYN (Position-Independent Executable)`, your target JSON's
`position-independent-executables`/`relocation-model` settings didn't take —
revisit Chapter 01. (This is exactly the bug Chapter 08 dissects.)

## Checkpoint

You have a kernel that builds and boots into an infinite loop. Boring on screen,
but the hard part — convincing the toolchain to make a bare-metal binary — is
done. Now let's make it *talk*.

---

Prev: [Chapter 01 — Toolchain & target](01-toolchain-and-target.md) ·
Next: [Chapter 03 — VGA text buffer →](03-vga-text-buffer.md)
