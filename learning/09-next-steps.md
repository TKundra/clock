# Chapter 09 — Where to Go Next

You have a working, *ticking* bare-metal clock. Here are the natural extensions,
roughly in order of difficulty, each one teaching a new core OS concept. They
build on each other.

## 1. Make the clock interrupt-driven (drop the polling loop)

Our clock is realtime, but it gets there by **busy-polling** the RTC in a loop
(Chapter 07). That works, but it pins the CPU spinning between ticks. The proper
OS way is to let the hardware *tell us* when a second has passed via an
**interrupt**, and `hlt` (sleep) the rest of the time.

What you'll learn:

- **The IDT (Interrupt Descriptor Table):** a table telling the CPU which
  function to call for each interrupt/exception. You build one and load it with
  `lidt`.
- **CPU exceptions first:** start by handling the breakpoint exception (`int3`)
  so you can prove your IDT works safely.
- **The PIC (8259):** the legacy interrupt controller. You remap it (its default
  vectors collide with CPU exceptions) and unmask the line you want.
- **A timer source:** either the **PIT** (programmable interval timer) firing at
  a fixed rate, or — most fitting here — the **RTC's own periodic/update-ended
  interrupt** (Status Register B/C, IRQ 8). On each interrupt, re-read the RTC
  and call the `ui::draw_dynamic` you already wrote.

The payoff: replace `run_clock`'s poll-and-spin loop with `loop { hlt() }`, and
do the redraw inside the interrupt handler. Same UI, but the CPU idles between
ticks instead of spinning — and you've built the machinery the next two steps
need.

> The blog_os ["CPU Exceptions"](https://os.phil-opp.com/cpu-exceptions/) and
> ["Hardware Interrupts"](https://os.phil-opp.com/hardware-interrupts/) posts
> map almost 1:1 onto this step. **Caution:** once an interrupt handler also
> draws to the screen, it can fire *while* `run_clock` holds the `WRITER`
> spinlock — an instant deadlock. You'll need to disable interrupts while
> holding that lock (the `x86_64::instructions::interrupts::without_interrupts`
> helper).

## 2. A real keyboard-driven shell

Make the clock one mode of an interactive shell — type `date` for a one-shot,
`clock` for the live panel, `help`, `clear`. (Builds directly on the IDT/PIC
machinery from step 1.)

What you'll learn:

- **PS/2 keyboard interrupt (IRQ 1):** on each keypress the controller fires an
  interrupt; you read the scancode from port `0x60`.
- **Scancode → character decoding:** the `pc-keyboard` crate handles scancode
  sets and modifier keys, or you can write a small US-layout table yourself.
- **A line editor + dispatcher:** accumulate typed characters into a buffer
  until Enter, then match the line against command names. The one-shot
  `cmd_date` from Chapter 06 becomes one branch; the live `run_clock` from
  Chapter 07 becomes another.

## 3. Better time handling

Small, self-contained improvements to the RTC driver:

- **Century register (`0x32`):** instead of hardcoding `+2000`, read the
  ACPI-defined century register where available (check the FADT's "century"
  field to know if it's valid) for a non-hardcoded century.
- **Set the time:** writing the RTC registers (the reverse of reading) lets you
  implement a `setdate` command. Remember to convert *to* BCD and to set/clear
  the update-inhibit bit (Status Register B bit 7) around the write.
- **Uptime:** count timer ticks from step 1 to show seconds-since-boot, and
  compute elapsed time between two RTC reads.

## 4. Testing without a screen

Kernels are notoriously hard to test. The serial port (Chapter 04) is the key:

- Add the **`isa-debug-exit`** device (`-device isa-debug-exit,iobase=0xf4`) and
  a helper that writes a port to make QEMU *exit with a chosen code*. Now a test
  can run the kernel, print results to serial, and exit cleanly.
- Use Rust's **custom test framework** (`#![feature(custom_test_frameworks)]`)
  to run `#[test_case]` functions in the kernel and report pass/fail over
  serial. Then a unit test for `bcd_to_binary` and `day_of_week` runs *in the
  kernel*.

> blog_os ["Testing"](https://os.phil-opp.com/testing/) covers this end to end.

## 5. Polish

- **Hide the cursor / set the hardware cursor position** via the VGA CRTC
  registers (ports `0x3D4`/`0x3D5`).
- **Colorize** the clock (you already have the `Color` enum — give the time a
  bright color, errors red).
- **A double-buffered redraw** so the live clock never flickers.

## Suggested path

If you want one concrete next project: **do step 1 (make the clock
interrupt-driven) first.** It's the single biggest conceptual jump — interrupts
are the gateway to basically all of OS development — and you already have a
visible result (the ticking UI) to convert from polling to interrupts, so you
can directly compare the two. Step 2 (keyboard shell) is the natural follow-on
because it reuses the same IDT/PIC machinery.

---

Prev: [Chapter 08 — Build, boot & verify](08-build-boot-verify.md) ·
Back to: [README / index](README.md)

Happy hacking. 🕐
