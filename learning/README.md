# Building a Bare-Metal RTC Clock in Rust

A chapter-by-chapter guide to building a tiny `no_std` x86-64 kernel that boots
under QEMU and prints the **real wall-clock time** by reading the CMOS
Real-Time Clock directly over I/O ports.
