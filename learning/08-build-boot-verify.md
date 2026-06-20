# Chapter 08 — Build, Boot & Verify

Goal: turn the compiled kernel into a bootable disk image, run it in QEMU, and
confirm it prints the real time. This chapter also walks through the **one real
bug** you'll hit — because debugging it teaches you more than any happy path.

## Step 1: tell `bootimage` how to launch QEMU

We want QEMU's emulated RTC to reflect the host's local wall-clock time, so
`date` shows *real* time. Add a `bootimage` config to `Cargo.toml`:

```toml
# Cargo.toml
[package.metadata.bootimage]
# `base=localtime` makes the emulated CMOS/RTC reflect the host's local
# wall-clock time, so `date` reads real time. Drop it for UTC.
run-args = ["-rtc", "base=localtime"]
```

## Step 2: build the bootable image

```bash
cargo bootimage
```

This compiles the kernel, compiles the `bootloader` crate, and uses
`llvm-objcopy` to combine them into a single bootable disk image:

```
Created bootimage for `clock` at
  target/x86_64-clock/debug/bootimage-clock.bin
```

## Step 3: the first boot — and the bug

Run it:

```bash
qemu-system-x86_64 \
  -drive format=raw,file=target/x86_64-clock/debug/bootimage-clock.bin \
  -rtc base=localtime
```

If you're following along with the *built-in* `x86_64-unknown-none` target
instead of our custom JSON, the screen shows a red panic — **from the
bootloader, not our kernel**:

```
panicked at src/page_table.rs:105:
failed to map segment at Page[4KiB](0x0): ...
```

### Diagnosing it

The panic message is the giveaway: the bootloader is trying to map a kernel
segment located at virtual address **`0x0`** and failing. Inspect the kernel
ELF:

```bash
readelf -h target/x86_64-clock/debug/clock | grep Type
#   Type:  DYN (Position-Independent Executable file)   <-- the problem

readelf -l target/x86_64-clock/debug/clock | grep -A1 LOAD | head
#   LOAD ... VirtAddr 0x0000000000000000   <-- a load segment at 0x0
```

The modern built-in bare-metal target builds a **PIE** (position-independent
executable). PIEs are designed to be relocated anywhere, so the linker places
the first segment at offset `0x0`. `bootloader` 0.9 expects a plain executable
linked at a fixed, non-zero address and chokes on the `0x0` segment.

> **How we actually found this headlessly:** with `-display none` the screen was
> invisible, and serial output was empty (the kernel never ran — the *bootloader*
> died first). So we read the VGA text memory directly through QEMU's monitor:
> ```bash
> qemu-system-x86_64 ... -display none \
>   -monitor unix:mon.sock,server,nowait &
> echo "xp/320xb 0xb8000" | socat -t3 - unix-connect:mon.sock
> ```
> Decoding the even bytes (ASCII) of that dump spelled out the
> "failed to map segment" panic — pointing straight at the bootloader. A good
> reminder that when serial is silent, the screen memory still holds the truth.

### The fix

Use a custom target that produces a **non-PIE, statically-relocated** executable
linked at `0x200000`. That's exactly the `x86_64-clock.json` from
[Chapter 01](01-toolchain-and-target.md): the lines that matter are

```json
"position-independent-executables": false,
"relocation-model": "static",
"relro-level": "off",
```

Rebuild and re-check:

```bash
cargo build
readelf -h target/x86_64-clock/debug/clock | grep -E 'Type|Entry'
#   Type:  EXEC (Executable file)        <-- fixed!
#   Entry point address:  0x2089d0       <-- in the 0x200000 range
```

(Chapter 01 already has you using this target, so if you followed in order you
*skip* the bug entirely — but now you know what it looks like and why.)

## Step 4: boot it for real

```bash
cargo run        # builds, makes the image, launches QEMU with our run-args
```

A QEMU window opens with the live clock panel, ticking once per second:

```
            ╔════════════════════════════════════════════════╗
            ║                                                  ║
            ║             BARE-METAL  RTC  CLOCK               ║
            ╠════════════════════════════════════════════════╣
            ║                                                  ║
            ║              Friday, 19 June 2026                ║
            ║                                                  ║
            ║                  16 : 00 : 28                    ║
            ║                                                  ║
            ║              2026-06-19T16:00:28                 ║
            ║                                                  ║
            ║      live  reading CMOS RTC @ 0x70/0x71 •        ║
            ╚════════════════════════════════════════════════╝
```

The big time advances every second and the `•` heartbeat blinks on each tick.

## Step 5: verify headlessly (and prove it's *real* time)

The screen is a *drawn* UI, so it doesn't come over the serial line — but our
per-tick `serial_println!` does. Pipe serial to stdout and you get a ticking
log. The kernel runs forever, so cap it with `timeout`:

```bash
echo "host start: $(date '+%H:%M:%S')"

timeout 6 qemu-system-x86_64 \
  -drive format=raw,file=target/x86_64-clock/debug/bootimage-clock.bin \
  -rtc base=localtime -no-reboot \
  -display none -serial stdio
```

Expected: the log advances exactly one second per line, in lock-step with the
host clock. That advancing match is the proof we're reading the real clock
hardware, not a hardcoded value:

```
host start: 15:59:47
clock kernel — live bare-metal RTC clock
2026-06-19T15:59:47
2026-06-19T15:59:48
2026-06-19T15:59:49
2026-06-19T15:59:50
2026-06-19T15:59:51
2026-06-19T15:59:52
```

### Bonus: snapshot the rendered screen

To verify the *drawn* UI headlessly, read VGA memory through the QEMU monitor
(this is the same trick we used to diagnose the bootloader panic). Each cell is
two bytes — `[ascii, color]` — so the even bytes spell out the screen:

```bash
qemu-system-x86_64 ... -display none \
  -monitor unix:mon.sock,server,nowait &
sleep 3
echo "xp/4000xb 0xb8000" | socat -t6 - unix-connect:mon.sock > dump.txt
# decode even bytes of dump.txt, 80 per row → see the box and time
```

## QEMU flags cheat-sheet

| Flag | Purpose |
|------|---------|
| `-drive format=raw,file=…bin` | Boot from our disk image |
| `-rtc base=localtime` | RTC reflects host local time (omit → UTC) |
| `-serial stdio` | Pipe COM1 to your terminal (text capture) |
| `-display none` | Headless — no GUI window |
| `-no-reboot` | Don't loop-reboot on a triple fault (so you can read the error) |
| `-d int,cpu_reset -D log.txt` | Log interrupts/resets — spot triple faults |
| `-monitor unix:mon.sock,server,nowait` | QEMU monitor on a socket (e.g. for `xp` memory dumps) |

## Checkpoint

You have a bootable bare-metal clock that draws a live UI and ticks in real
time, verified second-by-second against the host. The whole project is done. The
final chapter points at where to take it next.

---

Prev: [Chapter 07 — A realtime clock with a UI](07-realtime-clock-ui.md) ·
Next: [Chapter 09 — Where to go next →](09-next-steps.md)
