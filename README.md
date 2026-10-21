# esp-hal direct-boot starter

A minimal [esp-hal](https://github.com/esp-rs/esp-hal) application that is started **directly by
the ROM** — no ESP-IDF 2nd-stage bootloader, no partition table, no app image header. It is meant
as a test bed to verify that esp-hal works without relying on the IDF bootloader.

## How direct boot works

If the ROM finds the magic word `0xaedb041d` twice in a row at a fixed flash offset, it maps the
start of flash into the instruction (and data) cache and jumps to the instruction directly after
the magic. Nothing is loaded into RAM — that's the application's job. Also see the C example in
[espressif/esp32c3-direct-boot-example](https://github.com/espressif/esp32c3-direct-boot-example).

This project therefore uses its own linker scripts instead of esp-hal's `linkall.x`:

- Every output section gets a load address (LMA) equal to its flash offset, and the code/rodata
  run address is `IROM_BASE`/`DROM_BASE` + flash offset, so flash executes in place.
- `.rwtext` (`#[ram]` code, trap handlers), `.data` and the RTC sections are placed in flash
  after `.rodata` and described by a copy table the linker generates at the start of `.rodata`.
- A small entry stub (`firmware/src/direct_boot.rs`) sits right after the magic. It copies each
  table entry from flash to RAM, runs a chip-specific `early_init` (see below), then jumps to the
  regular runtime entry (`_start` of `riscv-rt`, `Reset` of `xtensa-lx-rt`), which zeroes `.bss`
  and calls `main`.
- esp-hal's generated `hal-defaults.x` and the ROM symbols from `esp-rom-sys` are reused.

## Chip support

| Chip     | Magic offset | Tested on hardware | Notes                                                  |
| -------- | ------------ | ------------------ | ------------------------------------------------------ |
| ESP32-C2 | `0x0`        | yes                | Watchdog not disabled (see below)                      |
| ESP32-C3 | `0x0`        | yes                | Requires chip revision v0.3 or later; watchdog not disabled |
| ESP32-C5 | `0x0`        | yes (v1.0)         | Needs the PMA setup                                    |
| ESP32-C6 | `0x0`        | yes                |                                                        |
| ESP32-C61| `0x0`        | yes                | Needs the PMA setup                                    |
| ESP32-H2 | `0x0`        | yes                |                                                        |
| ESP32-H4 | `0x0`        | no                 | No public TRM, memory layout taken from esp-hal        |
| ESP32-P4 | `0x0`        | yes (v3.1)         | Needs the PMA setup and a cache write-back; watchdog not disabled |
| ESP32-S3 | `0x400`      | yes (v0.1)         | The ROM jumps to `0x42000408`; needs the cache idle wait; watchdog not disabled |
| ESP32-S31| `0x0`        | yes                | Needs the PMA setup; memory layout taken from esp-hal  |

"Tested" means the firmware boots and all checks below pass. Not supported by the hardware:
ESP32 and ESP32-S2.

## What `early_init` does instead of the bootloader

The ROM leaves the chip in a state the 2nd-stage bootloader normally cleans up before esp-hal
starts. Everything below runs before the runtime, so it can't use statics.

- **PMA (C5, C61, P4, S31):** the ROMs of these chips leave Physical Memory Attribute entries
  enabled (on the C5 e.g. all of SRAM as non-executable), so the first `#[ram]` code or trap
  locks the CPU up. `early_init` replaces them like ESP-IDF's
  `esp_cpu_configure_invalid_regions()`, just from flash, so the entries are set in an order that
  keeps flash covered. On the P4 the uncached L2MEM alias is assumed to be at offset `0x40000000`.
- **P4 cache write-back:** internal RAM sits behind the L1 caches. The copy loop leaves the
  copied code in the write-back data cache, where instruction fetches don't see it, so the first
  trap jumps into stale memory. `early_init` writes back the data cache and invalidates the
  instruction cache for every copied section.
- **S3 cache idle wait:** the ROM jumps to the entry with cache operations still in flight.
  esp-hal reconfigures the caches right away through the ROM's `Cache_Suspend_DCache`, which
  returns before the cache is idle (ESP-IDF patches it to wait), and the data cache then hangs on
  the next miss. `early_init` waits until both caches report idle.

## Known gaps in esp-hal

- **Watchdogs (C2, C3, P4, S3):** `esp_hal::init` doesn't disable every watchdog the ROM arms for
  flash boot, so these chips get a watchdog reset a few seconds into the `Bing!` loop
  (`TG0WDT_SYS_RST` / `HP_SYS_HP_WDT_RESET`). With the bootloader this isn't visible because the
  bootloader reconfigures them. This is left as is on purpose: finding these gaps is the point of
  the project.

## Prerequisites

- Rust stable with the RISC-V targets:
  `rustup target add riscv32imc-unknown-none-elf riscv32imac-unknown-none-elf riscv32imafc-unknown-none-elf`
- For the ESP32-S3, the `esp` Xtensa toolchain installed via [espup](https://github.com/esp-rs/espup).
- [espflash](https://github.com/esp-rs/espflash) 4.x: `cargo install espflash --locked`

esp-hal, esp-println and esp-backtrace are git dependencies of the esp-hal repository, pinned to
commit [`286a7e8`](https://github.com/esp-rs/esp-hal/commit/286a7e86a9ce88411a45fb9cd0b28dd431960d01)
(the revision the hardware results above were obtained with). To try a newer esp-hal, change the
`rev` of all three in `firmware/Cargo.toml`.

## Usage

```sh
cargo xtask build esp32c6            # build + create the flash image
cargo xtask run esp32c6 [-p COM5]    # build, flash at 0x0 and open the monitor
cargo xtask build-all                # build every chip
```

Add `--debug` to use the dev profile, or `--trace` to enable the `boot-trace` feature (see
[Debugging](#debugging)). The ELF and the raw image end up in
`firmware/target/images/<profile>/<chip>.{elf,bin}`. The image is flashed as is at offset `0x0`
(for the S3 its first `0x400` bytes are padding).

The firmware prints a hello message and checks the parts direct boot is responsible for:

```text
Hello from direct boot!
.data @ 0x... = 0xc0ffee42, .bss @ 0x... = 0x00000000
[ OK ] .data copied from flash
[ OK ] .bss zeroed
#[ram] function @ 0x...
[ OK ] .rwtext copied from flash
[ OK ] software interrupt handled (vectors in RAM)
Bing!
```

## Debugging

`--trace` prints through the ROM's `esp_rom_printf`, so it works before esp-hal is initialized
and needs nothing but the serial port:

- on RISC-V, `mtvec`, `mstatus`, the PMP configuration and the PMA entries as left by the ROM and
  after the PMA setup, plus a probe that executes code from RAM;
- on all chips, markers before and after `esp_hal::init`, which tell a hang in the runtime
  startup from one in esp-hal.

When the trace isn't enough, a debugger finds hangs fastest. With a USB-JTAG connection
(ESP-IDF's `openocd-esp32`):

```sh
openocd -f board/esp32p4-builtin.cfg -c "init; halt; echo [get_reg pc]; echo [get_reg mcause]; shutdown"
```

Symbolize the PC with `llvm-addr2line` (or `xtensa-esp32s3-elf-addr2line`) on the ELF in
`firmware/target/images/`. If a hang disappears when you halt the core with breakpoints, look for
a race; on the S3 a debugger halt also completes pending cache operations.

## Layout

- `firmware/` — the application (one cargo feature per chip).
  - `ld/<chip>/direct_boot.x` — per-chip memory map, flash/cache bases, includes the shared layout.
  - `ld/riscv/`, `ld/xtensa/` — the shared section layout and copy table.
  - `src/direct_boot.rs` — the entry stubs, `early_init` and the `boot-trace` dumps.
- `xtask/` — builds the firmware, converts the ELF into a flash image (sections placed at their
  load addresses, gaps filled with `0xFF`, magic checked) and flashes it with `espflash write-bin`.

## Caveats

- No partition table, OTA, app descriptor or secure boot — there is no bootloader to use them.
- Direct boot can be disabled permanently via eFuse (`DIS_DIRECT_BOOT`); it must not be burned.
- When switching from an IDF-style image, run `espflash erase-flash` once, so a stale
  bootloader or partition table can't get in the way.
- The ROM only maps the first 4 MB of flash; the xtask refuses larger images.
- esp-hal's PMP setup protects `.rwtext`; the RISC-V scripts keep the CLIC vector table (written at
  runtime) in front of `_rwtext_start` for that reason.
- The ESP32-H4 image is untested; treat it as a starting point.
