//! Entry point the ROM jumps to in direct-boot mode (flash offset of the magic + 8).
//!
//! Without a 2nd-stage bootloader nothing loads the RAM sections, so the stub copies every entry
//! of the linker-generated copy table (`.rwtext`, `.data`, RTC memory, ...) from flash to RAM and
//! then continues with the regular runtime startup. The copy loop runs without a stack.
//!
//! Copy table entries are `(source in flash, destination start, destination end)` word triples,
//! see `ld/*/direct_boot_*.x`.
//!
//! On RISC-V, [`riscv::early_init`] then takes over the hardware setup the 2nd-stage bootloader
//! would otherwise have done, before jumping to `_start` of `riscv-rt`.

/// `boot-trace`: print a progress marker through the ROM, which needs no initialized HAL.
#[cfg(feature = "boot-trace")]
pub fn mark(msg: &core::ffi::CStr) {
    unsafe extern "C" {
        fn esp_rom_printf(fmt: *const core::ffi::c_char, ...) -> i32;
    }
    unsafe { esp_rom_printf(c"[direct-boot] %s\n".as_ptr(), msg.as_ptr()) };
}

#[cfg(target_arch = "riscv32")]
core::arch::global_asm!(
    r#"
    .pushsection .direct_boot.entry, "ax"
    .global _direct_boot_entry
    .type _direct_boot_entry, @function
    .option push
    .option norelax
_direct_boot_entry:
    la   t0, _direct_boot_copy_table_start
    la   t1, _direct_boot_copy_table_end
1:
    bgeu t0, t1, 3f
    lw   t2, 0(t0)
    lw   t3, 4(t0)
    lw   t4, 8(t0)
    addi t0, t0, 12
2:
    bgeu t3, t4, 1b
    lw   t5, 0(t2)
    sw   t5, 0(t3)
    addi t2, t2, 4
    addi t3, t3, 4
    j    2b
3:
    fence
    fence.i
    la   sp, _stack_start
    call {early_init}
    la   t0, _start
    jr   t0
    .option pop
    .size _direct_boot_entry, . - _direct_boot_entry
    .popsection
"#,
    early_init = sym riscv::early_init,
);

#[cfg(target_arch = "riscv32")]
mod riscv {
    #[cfg(any(feature = "boot-trace", rom_pma))]
    use core::arch::asm;

    /// Runs from flash with a stack but before `riscv-rt` initialized anything else: `.bss` is not
    /// zeroed yet and `gp` is not set up, so this must not touch any statics.
    pub(super) extern "C" fn early_init() {
        #[cfg(feature = "esp32p4")]
        cache::sync_copied_sections();

        #[cfg(feature = "boot-trace")]
        trace::dump(c"as left by the ROM");

        #[cfg(rom_pma)]
        pma::configure();

        #[cfg(all(feature = "boot-trace", rom_pma))]
        trace::dump(c"after PMA setup");
    }

    /// The internal SRAM of the ESP32-P4 sits behind the L1 caches: the copy loop leaves the code
    /// it copied in the (write-back) data cache, where instruction fetches don't see it.
    #[cfg(feature = "esp32p4")]
    mod cache {
        const CACHE_MAP_L1_ICACHE_0: u32 = 1 << 0;
        const CACHE_MAP_L1_ICACHE_1: u32 = 1 << 1;
        const CACHE_MAP_L1_DCACHE: u32 = 1 << 4;

        unsafe extern "C" {
            fn Cache_WriteBack_Addr(map: u32, addr: u32, size: u32);
            fn Cache_Invalidate_Addr(map: u32, addr: u32, size: u32);
            static _direct_boot_copy_table_start: [u32; 3];
            static _direct_boot_copy_table_end: [u32; 3];
        }

        pub(super) fn sync_copied_sections() {
            let mut entry = &raw const _direct_boot_copy_table_start;
            while entry < &raw const _direct_boot_copy_table_end {
                unsafe {
                    let [_, start, end] = entry.read();
                    if end > start {
                        Cache_WriteBack_Addr(CACHE_MAP_L1_DCACHE, start, end - start);
                        Cache_Invalidate_Addr(
                            CACHE_MAP_L1_ICACHE_0 | CACHE_MAP_L1_ICACHE_1,
                            start,
                            end - start,
                        );
                    }
                    entry = entry.add(1);
                }
            }
        }
    }

    #[cfg(feature = "boot-trace")]
    pub(super) fn read_csr<const CSR: u16>() -> u32 {
        let value: u32;
        unsafe { asm!("csrr {0}, {csr}", out(reg) value, csr = const CSR) };
        value
    }

    #[cfg(rom_pma)]
    pub(super) fn write_csr<const CSR: u16>(value: u32) {
        unsafe { asm!("csrw {csr}, {0}", in(reg) value, csr = const CSR) };
    }

    /// The 16 consecutive CSRs starting at `$base`, accessed through `$f`.
    #[cfg(any(
        rom_pma,
        all(feature = "boot-trace", not(any(feature = "esp32c2", feature = "esp32c3")))
    ))]
    macro_rules! csr_array {
        ($f:ident, $base:literal) => {
            [
                $f::<{ $base }>, $f::<{ $base + 1 }>, $f::<{ $base + 2 }>, $f::<{ $base + 3 }>,
                $f::<{ $base + 4 }>, $f::<{ $base + 5 }>, $f::<{ $base + 6 }>,
                $f::<{ $base + 7 }>, $f::<{ $base + 8 }>, $f::<{ $base + 9 }>,
                $f::<{ $base + 10 }>, $f::<{ $base + 11 }>, $f::<{ $base + 12 }>,
                $f::<{ $base + 13 }>, $f::<{ $base + 14 }>, $f::<{ $base + 15 }>,
            ]
        };
    }

    /// Physical memory attributes. The ROM of these chips leaves some PMA entries enabled (e.g.
    /// all of SRAM as R/W only), which the 2nd-stage bootloader replaces. Mirrors
    /// `esp_cpu_configure_invalid_regions()` in ESP-IDF's `cpu_region_protect.c`: block the
    /// address space gaps, keep ROM RX and flash RWX + cacheable, and leave everything else
    /// (SRAM, LP memory, peripherals, CPU subsystem) uncovered. Values are `soc.h` constants.
    #[cfg(rom_pma)]
    mod pma {
        use super::write_csr;

        const EN: u32 = 1 << 0;
        const X: u32 = 1 << 2;
        const W: u32 = 1 << 3;
        const R: u32 = 1 << 4;
        const L: u32 = 1 << 29;
        const TOR: u32 = 0x4000_0000;
        const NAPOT: u32 = 0xC000_0000;

        const NONE: u32 = L | EN;
        #[cfg(any(feature = "esp32c5", feature = "esp32s31"))]
        const RX: u32 = L | EN | R | X;
        #[cfg(feature = "esp32s31")]
        const RW: u32 = L | EN | R | W;
        const RWX: u32 = L | EN | R | W | X;

        const fn tor(addr: u32) -> u32 {
            addr >> 2
        }

        const fn napot(base: u32, size: u32) -> u32 {
            (base | ((size >> 1) - 1)) >> 2
        }

        /// `(pmaaddr, pmacfg)` for entries 0..16.
        #[cfg(feature = "esp32c5")]
        const ENTRIES: [(u32, u32); 16] = {
            const CPU_SUBSYSTEM_LOW: u32 = 0x2000_0000;
            const CPU_SUBSYSTEM_HIGH: u32 = 0x3000_0000;
            const IROM_MASK_LOW: u32 = 0x4000_0000;
            const DROM_MASK_HIGH: u32 = 0x4005_0000;
            const IRAM_LOW: u32 = 0x4080_0000;
            const IRAM_HIGH: u32 = 0x4086_0000;
            const IROM_LOW: u32 = 0x4200_0000;
            const IROM_HIGH: u32 = 0x4400_0000;
            const RTC_IRAM_LOW: u32 = 0x5000_0000;
            const RTC_IRAM_HIGH: u32 = 0x5000_4000;
            const PERIPHERAL_LOW: u32 = 0x6000_0000;
            const PERIPHERAL_HIGH: u32 = 0x6010_0000;
            [
                // gap at the bottom of the address space
                (napot(0, CPU_SUBSYSTEM_LOW), NAPOT | NONE),
                // gap between CPU subsystem and ROM
                (tor(CPU_SUBSYSTEM_HIGH), NONE),
                (tor(IROM_MASK_LOW), TOR | NONE),
                // ROM: RX, cacheable
                (tor(IROM_MASK_LOW), NONE),
                (tor(DROM_MASK_HIGH), TOR | RX),
                // gap between SRAM and flash
                (tor(IRAM_HIGH), NONE),
                (tor(IROM_LOW), TOR | NONE),
                // flash (and PSRAM): RWX, cacheable
                (napot(IROM_LOW, IROM_HIGH - IROM_LOW), NAPOT | RWX),
                // gap between flash and LP memory
                (tor(IROM_HIGH), NONE),
                (tor(RTC_IRAM_LOW), TOR | NONE),
                // end of the address space
                (tor(PERIPHERAL_HIGH), NONE),
                (tor(u32::MAX), TOR | NONE),
                // gap between ROM and SRAM
                (tor(DROM_MASK_HIGH), NONE),
                (tor(IRAM_LOW), TOR | NONE),
                // gap between LP memory and peripherals
                (tor(RTC_IRAM_HIGH), NONE),
                (tor(PERIPHERAL_LOW), TOR | NONE),
            ]
        };

        #[cfg(feature = "esp32c61")]
        const ENTRIES: [(u32, u32); 16] = {
            const CPU_SUBSYSTEM_LOW: u32 = 0x2000_0000;
            const CPU_SUBSYSTEM_HIGH: u32 = 0x3000_0000;
            const IROM_MASK_LOW: u32 = 0x4000_0000;
            const DROM_MASK_HIGH: u32 = 0x4004_0000;
            const IRAM_LOW: u32 = 0x4080_0000;
            const IRAM_HIGH: u32 = 0x4085_0000;
            const IROM_LOW: u32 = 0x4200_0000;
            const IROM_HIGH: u32 = 0x4400_0000;
            const PERIPHERAL_LOW: u32 = 0x6000_0000;
            const PERIPHERAL_HIGH: u32 = 0x6010_0000;
            [
                // gap at the bottom of the address space
                (napot(0, CPU_SUBSYSTEM_LOW), NAPOT | NONE),
                // gap between CPU subsystem and ROM
                (tor(CPU_SUBSYSTEM_HIGH), NONE),
                (tor(IROM_MASK_LOW), TOR | NONE),
                // gap between ROM and SRAM
                (tor(DROM_MASK_HIGH), NONE),
                (tor(IRAM_LOW), TOR | NONE),
                // gap between SRAM and flash
                (tor(IRAM_HIGH), NONE),
                (tor(IROM_LOW), TOR | NONE),
                // flash (and PSRAM): RWX, cacheable
                (napot(IROM_LOW, IROM_HIGH - IROM_LOW), NAPOT | RWX),
                // gap between flash and peripherals
                (tor(IROM_HIGH), NONE),
                (tor(PERIPHERAL_LOW), TOR | NONE),
                // end of the address space
                (tor(PERIPHERAL_HIGH), NONE),
                (tor(u32::MAX), TOR | NONE),
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 0),
            ]
        };

        #[cfg(feature = "esp32p4")]
        const ENTRIES: [(u32, u32); 16] = {
            const CPU_SUBSYSTEM_LOW: u32 = 0x2000_0000;
            const CPU_SUBSYSTEM_HIGH: u32 = 0x3000_0000;
            const SPM_LOW: u32 = 0x3010_0000;
            const SPM_HIGH: u32 = 0x3010_2000;
            const CPU_PERIPH_LOW: u32 = 0x3FF0_0000;
            const CPU_PERIPH_HIGH: u32 = 0x3FF2_0000;
            const IROM_LOW: u32 = 0x4000_0000;
            const DROM_HIGH: u32 = 0x4400_0000;
            const EXTRAM_LOW: u32 = 0x4800_0000;
            const EXTRAM_HIGH: u32 = 0x4C00_0000;
            const IROM_MASK_LOW: u32 = 0x4FC0_0000;
            const IROM_MASK_HIGH: u32 = 0x4FC2_0000;
            const IRAM_LOW: u32 = 0x4FF0_0000;
            const DRAM_HIGH: u32 = 0x4FFC_0000;
            const PERIPHERAL_LOW: u32 = 0x5000_0000;
            /// `CACHE_LL_L2MEM_NON_CACHE_ADDR`: L2MEM + `SOC_NON_CACHEABLE_OFFSET_SRAM`.
            const L2MEM_NON_CACHE_OFFSET: u32 = 0x4000_0000;
            [
                // gap at the bottom of the address space
                (napot(0, CPU_SUBSYSTEM_LOW), NAPOT | NONE),
                // gap between CPU subsystem and HP SPM
                (tor(CPU_SUBSYSTEM_HIGH), NONE),
                (tor(SPM_LOW), TOR | NONE),
                // gap between HP SPM and CPU peripherals
                (tor(SPM_HIGH), NONE),
                (tor(CPU_PERIPH_LOW), TOR | NONE),
                // gap between CPU peripherals and flash
                (tor(CPU_PERIPH_HIGH), NONE),
                (tor(IROM_LOW), TOR | NONE),
                // gap between flash and PSRAM
                (napot(DROM_HIGH, EXTRAM_LOW - DROM_HIGH), NAPOT | NONE),
                // gap between PSRAM and ROM
                (tor(EXTRAM_HIGH), NONE),
                (tor(IROM_MASK_LOW), TOR | NONE),
                // gap between ROM and L2MEM
                (tor(IROM_MASK_HIGH), NONE),
                (tor(IRAM_LOW), TOR | NONE),
                // gap between L2MEM and HP peripherals
                (napot(DRAM_HIGH, PERIPHERAL_LOW - DRAM_HIGH), NAPOT | NONE),
                // flash, PSRAM, ROM and L2MEM: RWX, cacheable (ROM uses entry 15 for this)
                (napot(IROM_LOW, PERIPHERAL_LOW - IROM_LOW), NAPOT | RWX),
                // gap between the uncached L2MEM alias and the end of the address space
                (tor(DRAM_HIGH + L2MEM_NON_CACHE_OFFSET), NONE),
                (tor(u32::MAX), TOR | NONE),
            ]
        };

        #[cfg(feature = "esp32s31")]
        const ENTRIES: [(u32, u32); 16] = {
            const CPU_SUBSYSTEM_LOW: u32 = 0x1000_0000;
            const RTC_IRAM_HIGH: u32 = 0x2E00_8000;
            const IRAM_LOW: u32 = 0x2F00_0000;
            const IRAM_HIGH: u32 = 0x2F08_0000;
            const IROM_MASK_LOW: u32 = 0x2F80_0000;
            const DROM_MASK_HIGH: u32 = 0x2F85_0000;
            const IROM_LOW: u32 = 0x4000_0000;
            const IROM_HIGH: u32 = 0x4400_0000;
            const EXTRAM_LOW: u32 = 0x5000_0000;
            const EXTRAM_HIGH: u32 = 0x5400_0000;
            const NON_CACHEABLE_OFFSET_FLASH: u32 = 0x6000_0000;
            const NON_CACHEABLE_OFFSET_PSRAM: u32 = 0x7000_0000;
            [
                // gap at the bottom of the address space
                (napot(0, CPU_SUBSYSTEM_LOW), NAPOT | NONE),
                // gap between LP TCM and HP TCM (internal SRAM)
                (tor(RTC_IRAM_HIGH), NONE),
                (tor(IRAM_LOW), TOR | NONE),
                // gap between internal SRAM and ROM
                (tor(IRAM_HIGH), NONE),
                (tor(IROM_MASK_LOW), TOR | NONE),
                // ROM: RX, cacheable
                (tor(DROM_MASK_HIGH), TOR | RX),
                // gap between ROM and flash
                (tor(IROM_LOW), TOR | NONE),
                // flash: RX, cacheable
                (tor(IROM_HIGH), TOR | RX),
                // gap between flash and PSRAM
                (tor(EXTRAM_LOW), TOR | NONE),
                // PSRAM: RWX, cacheable
                (tor(EXTRAM_HIGH), TOR | RWX),
                // gap between PSRAM and the cache-bypass alias of flash
                (tor(IROM_LOW + NON_CACHEABLE_OFFSET_FLASH), TOR | NONE),
                // cache-bypass alias of flash
                (tor(IROM_HIGH + NON_CACHEABLE_OFFSET_FLASH), TOR | RX),
                // gap between the flash and PSRAM cache-bypass aliases
                (tor(EXTRAM_LOW + NON_CACHEABLE_OFFSET_PSRAM), TOR | NONE),
                // cache-bypass alias of PSRAM
                (tor(EXTRAM_HIGH + NON_CACHEABLE_OFFSET_PSRAM), TOR | RW),
                // end of the address space
                (tor(u32::MAX), TOR | NONE),
                (0, 0),
            ]
        };

        /// Runs from flash, so unlike ESP-IDF (which runs this from RAM) each entry is reset just
        /// before it's set: flash is covered by the new entries before the ROM's flash entry
        /// (one of the last ones) is replaced, and lower entries take priority.
        pub(super) fn configure() {
            let cfg: [fn(u32); 16] = csr_array!(write_csr, 0xBC0);
            let addr: [fn(u32); 16] = csr_array!(write_csr, 0xBD0);
            for (i, &(entry_addr, entry_cfg)) in ENTRIES.iter().enumerate() {
                cfg[i](0);
                addr[i](0);
                addr[i](entry_addr);
                cfg[i](entry_cfg);
            }
        }
    }

    /// `boot-trace`: dump the memory protection state and check that RAM code and atomics work.
    #[cfg(feature = "boot-trace")]
    mod trace {
        use core::ffi::{CStr, c_char};

        use super::read_csr;

        unsafe extern "C" {
            fn esp_rom_printf(fmt: *const c_char, ...) -> i32;
        }

        #[unsafe(link_section = ".rwtext")]
        #[inline(never)]
        extern "C" fn ram_probe() -> u32 {
            0x600d_c0de
        }

        pub(super) fn dump(stage: &CStr) {
            let pmpcfg: [fn() -> u32; 4] = [
                read_csr::<0x3A0>,
                read_csr::<0x3A1>,
                read_csr::<0x3A2>,
                read_csr::<0x3A3>,
            ];

            unsafe {
                esp_rom_printf(c"\n[direct-boot] --- %s ---\n".as_ptr(), stage.as_ptr());
                esp_rom_printf(c"[direct-boot] mtvec=%08x mstatus=%08x\n".as_ptr(),
                    read_csr::<0x305>(), read_csr::<0x300>());
                // Only the configuration: no ROM so far enables a PMP entry.
                esp_rom_printf(c"[direct-boot] pmpcfg0..3 = %08x %08x %08x %08x\n".as_ptr(),
                    pmpcfg[0](), pmpcfg[1](), pmpcfg[2](), pmpcfg[3]());

                // ESP32-C2/C3 have no PMA registers.
                #[cfg(not(any(feature = "esp32c2", feature = "esp32c3")))]
                {
                    let pmacfg: [fn() -> u32; 16] = csr_array!(read_csr, 0xBC0);
                    let pmaaddr: [fn() -> u32; 16] = csr_array!(read_csr, 0xBD0);
                    for i in 0..16 {
                        esp_rom_printf(
                            c"[direct-boot] pma%d: cfg=%08x addr=%08x (<<2 = %08x)\n".as_ptr(),
                            i as u32, pmacfg[i](), pmaaddr[i](), pmaaddr[i]() << 2);
                    }
                }

                esp_rom_printf(c"[direct-boot] probing RAM execution at %08x\n".as_ptr(),
                    ram_probe as *const () as u32);
                let result = ram_probe();
                esp_rom_printf(c"[direct-boot] RAM execution ok (%08x)\n".as_ptr(), result);
            }
        }
    }
}

// `Reset` (xtensa-lx-rt) and [`xtensa::early_init`] are windowed functions in `.rwtext`, so they
// can only be called once the copy is done. The register window state the ROM leaves behind is
// reset first so that their `entry` can't trigger a window overflow into ROM frames.
#[cfg(target_arch = "xtensa")]
core::arch::global_asm!(
    r#"
    .pushsection .direct_boot.entry, "ax"
    .global _direct_boot_entry
    .type _direct_boot_entry, @function
_direct_boot_entry:
    j    .Ldb_start

    .p2align 2
.Ldb_table_start:
    .word _direct_boot_copy_table_start
.Ldb_table_end:
    .word _direct_boot_copy_table_end
.Ldb_stack:
    .word _stack_start_cpu0
.Ldb_reset:
    .word Reset
.Ldb_early_init:
    .word {early_init}
.Ldb_ps:
    // PS.WOE | PS.UM, INTLEVEL 0
    .word 0x00040020

.Ldb_start:
    movi   a0, 0
    wsr.intenable a0
    rsync

    l32r   a2, .Ldb_table_start
    l32r   a3, .Ldb_table_end
.Ldb_next_entry:
    bgeu   a2, a3, .Ldb_copy_done
    l32i   a4, a2, 0
    l32i   a5, a2, 4
    l32i   a6, a2, 8
    addi   a2, a2, 12
.Ldb_copy_word:
    bgeu   a5, a6, .Ldb_next_entry
    l32i   a7, a4, 0
    s32i   a7, a5, 0
    addi   a4, a4, 4
    addi   a5, a5, 4
    j      .Ldb_copy_word

.Ldb_copy_done:
    memw

    movi   a0, 0
    wsr.windowbase a0
    rsync
    movi   a0, 1
    wsr.windowstart a0
    rsync
    l32r   a0, .Ldb_ps
    wsr.ps a0
    rsync

    l32r   a1, .Ldb_stack
    movi   a0, 0
    l32r   a8, .Ldb_early_init
    callx4 a8

    movi   a0, 0
    l32r   a8, .Ldb_reset
    callx4 a8
    .size _direct_boot_entry, . - _direct_boot_entry
    .popsection
"#,
    early_init = sym xtensa::early_init,
);

#[cfg(target_arch = "xtensa")]
mod xtensa {
    const EXTMEM_CACHE_STATE: *const u32 = 0x600C_4130 as *const u32;
    const ICACHE_STATE_IDLE: u32 = 1;
    const DCACHE_STATE_IDLE: u32 = 1 << 12;
    const CACHE_STATE_MASK: u32 = 0x00FF_FFFF;

    /// The ROM jumps to the entry point with cache operations still in flight. esp-hal
    /// reconfigures the caches right away through the ROM's `Cache_Suspend_DCache`, which returns
    /// before the cache is idle (ESP-IDF patches it to wait), and the data cache then hangs on the
    /// next miss.
    #[unsafe(link_section = ".rwtext")]
    pub(super) extern "C" fn early_init() {
        while unsafe { EXTMEM_CACHE_STATE.read_volatile() } & CACHE_STATE_MASK
            != ICACHE_STATE_IDLE | DCACHE_STATE_IDLE
        {}
    }
}
