/* ESP32-H4 (no public TRM yet, direct boot untested) */

/* Assumed: the ROM maps flash at 0x42000000 for both instruction and data access. */
DIRECT_BOOT_MAGIC_OFFSET = 0;
IROM_BASE = 0x42000000;
DROM_BASE = 0x42000000;
IRAM_DRAM_OFFSET = 0;

PROVIDE(_max_hart_id = 1);
PROVIDE(interrupt0 = DefaultHandler);

MEMORY
{
    RAM : ORIGIN = 0x40810000, LENGTH = 0x3F350

    /* used by the ROM during boot, free afterwards */
    dram2_seg ( RW ) : ORIGIN = ORIGIN(RAM) + LENGTH(RAM), len = 0x4085d350 - (ORIGIN(RAM) + LENGTH(RAM))

    ROM : ORIGIN = 0x42000000, LENGTH = 0x400000
}

REGION_ALIAS("ROTEXT", ROM);
REGION_ALIAS("RODATA", ROM);
REGION_ALIAS("RWTEXT", RAM);
REGION_ALIAS("RWDATA", RAM);

INCLUDE "direct_boot_riscv.x"
INCLUDE "direct_boot_riscv_no_rtc_fast.x"
INCLUDE "hal-defaults.x"
