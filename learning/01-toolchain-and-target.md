# Chapter 01 — Toolchain & Target

Goal: get a project that *compiles for bare metal*, even though it won't do
anything useful yet. This chapter is all configuration — but each file earns its
place.

## Step 1: Create the project

```bash
cargo new clock
cd clock
```

You get a normal `Cargo.toml` and a `src/main.rs` with "Hello, world!". We'll
replace both, but first the toolchain.

## Step 2: Pin nightly (`rust-toolchain.toml`)

Bare-metal needs **nightly** Rust, because we cross-compile the `core` library
ourselves with an unstable feature called `build-std`. Create
`rust-toolchain.toml` so anyone who builds the project gets the right toolchain
automatically:

```toml
# rust-toolchain.toml
[toolchain]
channel = "nightly"
components = ["rust-src", "llvm-tools-preview"]
```

Why each component:

- **`rust-src`** — the *source* of `core`/`compiler_builtins`, so we can compile
  them for our weird target (there's no prebuilt copy).
- **`llvm-tools-preview`** — provides `llvm-objcopy`, which the `bootimage` tool
  uses later to assemble the disk image.

`rustup` reads this file and installs everything on first use.

## Step 3: A custom target (`x86_64-clock.json`)

A *target* describes the machine we compile for. Rust ships a built-in bare-metal
target, `x86_64-unknown-none`, but it produces a **position-independent
executable (PIE)** that the `bootloader` 0.9 crate can't load (see Chapter 08
for the gory details). So we define our own, based on the built-in one but with
PIE turned off:

```json
// x86_64-clock.json
{
  "arch": "x86_64",
  "cpu": "x86-64",
  "crt-objects-fallback": "false",
  "data-layout": "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128",
  "disable-redzone": true,
  "executables": true,
  "features": "-mmx,-sse,-sse2,-sse3,-ssse3,-sse4.1,-sse4.2,-avx,-avx2,+soft-float",
  "linker": "rust-lld",
  "linker-flavor": "gnu-lld",
  "llvm-target": "x86_64-unknown-none-elf",
  "max-atomic-width": 64,
  "panic-strategy": "abort",
  "plt-by-default": false,
  "position-independent-executables": false,
  "relocation-model": "static",
  "relro-level": "off",
  "rustc-abi": "softfloat",
  "stack-probes": { "kind": "inline" },
  "target-pointer-width": 64
}
```

The fields that matter most for a kernel:

| Field | Why |
|-------|-----|
| `"disable-redzone": true` | The "red zone" is a 128-byte area below the stack pointer that leaf functions use without adjusting RSP. An interrupt handler would clobber it. Off for kernels. |
| `"features": "…-sse…,+soft-float"` | SSE/MMX use special registers that aren't saved on interrupt entry by default. We disable them and use software floating point. |
| `"panic-strategy": "abort"` | There's no stack unwinding without an OS. On panic, just stop. |
| `"position-independent-executables": false` + `"relocation-model": "static"` | Produce a plain `ET_EXEC` linked at a fixed address (`0x200000`). **This is the fix for the bootloader-can't-map bug.** |
| `"relro-level": "off"` | Avoids an extra read-only-relocation segment the bootloader doesn't need. |

> **Tip:** you can dump the built-in spec with
> `rustc +nightly -Z unstable-options --target x86_64-unknown-none --print target-spec-json`
> and diff it against ours to see exactly what we changed.

## Step 4: Cargo config (`.cargo/config.toml`)

This ties everything together so a plain `cargo build` just works:

```toml
# .cargo/config.toml

# Default to our custom target so we never have to type --target.
[build]
target = "x86_64-clock.json"

# No prebuilt core for this target — build it (and compiler_builtins) from
# source. compiler-builtins-mem provides memcpy/memset/etc. that core needs.
[unstable]
build-std = ["core", "compiler_builtins"]
build-std-features = ["compiler-builtins-mem"]
# Recent nightlies require this flag to allow a custom .json target spec.
json-target-spec = true

# `cargo run` hands the ELF to `bootimage`, which makes it bootable + runs QEMU.
# The table key is the target file's name without ".json".
[target.x86_64-clock]
runner = "bootimage runner"
```

> **Note:** `json-target-spec = true` is needed on newer nightlies (2025+). If
> your nightly complains it doesn't recognize the flag, you're on an older one
> and can remove that line.

## Step 5: Cargo.toml

For now, just the package stanza (we'll add dependencies as each chapter needs
them). Note `edition = "2021"` — edition 2024 changes how `#[no_mangle]` must be
written, and 2021 keeps the blog_os-style code below working as-is.

```toml
# Cargo.toml
[package]
name = "clock"
version = "0.1.0"
edition = "2021"

[dependencies]
# (added in later chapters)
```

## Step 6: Install the tooling

```bash
cargo install bootimage          # builds the bootable disk image
rustup component add llvm-tools-preview   # if not already pulled by toolchain file
```

## Checkpoint

You can't build yet (there's still a `std`-using `main.rs`), but you've set up:

- nightly + components,
- a custom bare-metal target with PIE disabled,
- `build-std` so `core` compiles for it,
- the `bootimage` runner.

Next we replace `main.rs` with a freestanding binary that actually compiles.

---

Prev: [Chapter 00 — Overview](00-overview.md) ·
Next: [Chapter 02 — The freestanding binary →](02-freestanding-binary.md)
