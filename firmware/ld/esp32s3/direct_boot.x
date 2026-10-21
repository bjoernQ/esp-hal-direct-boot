/* ESP32-S3 */

/* The S3 ROM expects the magic at flash offset 0x400 and jumps to 0x42000408.
   It maps the first 4 MB of flash to both IROM and DROM. */
DIRECT_BOOT_MAGIC_OFFSET = 0x400;
IROM_BASE = 0x42000000;
DROM_BASE = 0x3C000000;

/* IRAM 0x40378000 and DRAM 0x3FC88000 address the same SRAM word */
IRAM_DRAM_OFFSET = 0x40378000 - 0x3FC88000;

/*
 40370000 <- IRAM/Icache -> 40378000 <- D/IRAM (I) -> 403E0000
                            3FC88000 <- D/IRAM (D) -> 3FCF0000 <- DRAM/DCache -> 3FD00000

 The vectors are placed at 0x40378000, which is right after the largest (32 KB) instruction cache
 configuration and has a DRAM alias, so they can be copied like everything else.
*/
MEMORY
{
  vectors_seg ( RX )     : ORIGIN = 0x40378000, len = 0x400
  iram_seg ( RX )        : ORIGIN = 0x40378400, len = 0x403C2000 - 0x40378400

  dram_seg ( RW )        : ORIGIN = 0x3FC88000, len = 0x3FCDB700 - 0x3FC88000
  /* used by the ROM during boot, free afterwards */
  dram2_seg ( RW )       : ORIGIN = 0x3FCDB700, len = 0x3FCED710 - 0x3FCDB700

  /* the default 64 KB data cache leaves nothing to reclaim */
  dcache_reclaimed_seg ( RW ) : ORIGIN = 0x3FCF0000, len = 0x3FD00000 - 0x3FCF0000 - 0x10000

  irom_seg ( RX )        : ORIGIN = 0x42000000, len = 4M
  drom_seg ( R )         : ORIGIN = 0x3C000000, len = 4M

  rtc_fast_seg ( RWX )   : ORIGIN = 0x600fe000, len = 8k
  rtc_slow_seg ( RW )    : ORIGIN = 0x50000000, len = 8k
}

REGION_ALIAS("ROTEXT", irom_seg);
REGION_ALIAS("RODATA", drom_seg);
REGION_ALIAS("RWTEXT", iram_seg);
REGION_ALIAS("RWDATA", dram_seg);

INCLUDE "hal-defaults.x"
INCLUDE "direct_boot_xtensa.x"
