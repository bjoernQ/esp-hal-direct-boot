/* ESP32-P4 (direct boot untested) */

/* Assumed: the ROM maps flash at 0x40000000 for both instruction and data access. */
DIRECT_BOOT_MAGIC_OFFSET = 0;
IROM_BASE = 0x40000000;
DROM_BASE = 0x40000000;
IRAM_DRAM_OFFSET = 0;

PROVIDE(_max_hart_id = 1);
PROVIDE(interrupt0 = DefaultHandler);

MEMORY
{
    /* The low end of the 768 KB L2MEM is used as L2 cache. Without a 2nd-stage bootloader we
       don't know which split the ROM configured, so assume the largest one (512 KB cache).
       Top is fixed at 0x4FFAE000 to leave the ROM BSS/stack region untouched. */
    RAM : ORIGIN = 0x4FF80000, LENGTH = 0xADFC0 - 0x80000

    /* used by the ROM during boot, free afterwards */
    dram2_seg ( RW ) : ORIGIN = ORIGIN(RAM) + LENGTH(RAM), len = 0x4ffbefc0 - (ORIGIN(RAM) + LENGTH(RAM))

    ROM : ORIGIN = 0x40000000, LENGTH = 0x400000

    /* The first 0x100 bytes of LP SRAM are reserved for the rev 3.0 deep-sleep MSPI workaround stub. */
    RTC_FAST : ORIGIN = 0x50108000 + 0x100, LENGTH = 32K - 0x100
}

REGION_ALIAS("ROTEXT", ROM);
REGION_ALIAS("RODATA", ROM);
REGION_ALIAS("RWTEXT", RAM);
REGION_ALIAS("RWDATA", RAM);

INCLUDE "direct_boot_riscv.x"
INCLUDE "direct_boot_riscv_rtc_fast.x"
INCLUDE "hal-defaults.x"
