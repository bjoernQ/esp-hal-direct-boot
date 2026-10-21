/*
 * Direct-boot layout shared by all RISC-V chips.
 *
 * Expects the chip script to define:
 *   - regions ROTEXT, RODATA (flash, as mapped by the ROM), RWTEXT, RWDATA (SRAM) and dram2_seg
 *   - DIRECT_BOOT_MAGIC_OFFSET: flash offset of the 2x 0xaedb041d magic, the ROM jumps to +8
 *   - IROM_BASE / DROM_BASE: where the ROM maps flash offset 0 for instruction / data access
 *   - IRAM_DRAM_OFFSET: IRAM address minus DRAM address of the same SRAM word (0 if unified)
 *
 * Flash layout (LMA == flash offset, VMA == IROM_BASE/DROM_BASE + LMA):
 *   header | .text | .rodata (incl. copy table) | .rwtext | .data | [.rtc_fast.text | .rtc_fast.data]
 *
 * Nothing loads RAM for us, so every section that lives in RAM gets a flash LMA and an entry in
 * the copy table that the entry stub (src/direct_boot.rs) processes before jumping to `_start`.
 */

ENTRY(_direct_boot_entry)

SECTIONS {
  .direct_boot.header IROM_BASE + DIRECT_BOOT_MAGIC_OFFSET : AT(DIRECT_BOOT_MAGIC_OFFSET)
  {
    LONG(0xaedb041d)
    LONG(0xaedb041d)
  } > ROTEXT

  .text IROM_BASE + DIRECT_BOOT_MAGIC_OFFSET + 8 : AT(DIRECT_BOOT_MAGIC_OFFSET + 8)
  {
    _text_start = ABSOLUTE(.);
    /* Must be first: the ROM jumps to the instruction right after the magic. */
    KEEP(*(.direct_boot.entry));

    KEEP(*(.init));
    KEEP(*(.init.rust));
    KEEP(*(.text.abort));
    *(.literal .text .literal.* .text.*)
    . = ALIGN(4);
    _text_end = ABSOLUTE(.);
  } > ROTEXT

  /* Same flash, possibly seen through a different bus (C2/C3: DROM at 0x3C000000). */
  .rodata ALIGN(DROM_BASE + LOADADDR(.text) + SIZEOF(.text), 16) : AT(ADDR(.rodata) - DROM_BASE)
  {
    _rodata_start = ABSOLUTE(.);

    /* (source in flash, destination, destination end) word triples */
    _direct_boot_copy_table_start = ABSOLUTE(.);
    LONG(_db_rwtext_src) LONG(_db_rwtext_dst) LONG(_db_rwtext_end)
    LONG(_db_data_src) LONG(_db_data_dst) LONG(_db_data_end)
    LONG(_db_rtc_fast_text_src) LONG(_db_rtc_fast_text_dst) LONG(_db_rtc_fast_text_end)
    LONG(_db_rtc_fast_data_src) LONG(_db_rtc_fast_data_dst) LONG(_db_rtc_fast_data_end)
    _direct_boot_copy_table_end = ABSOLUTE(.);

    *(.rodata .rodata.*)
    *(.srodata .srodata.*)
    *(.rodata_wlog_*.*)
    . = ALIGN(4);
    _rodata_end = ABSOLUTE(.);
  } > RODATA

  /* .trap (vector table, CLIC mtvt tables, trap entry) followed by the `#[ram]` code */
  .rwtext ORIGIN(RWTEXT) : AT(LOADADDR(.rodata) + SIZEOF(.rodata))
  {
    _trap_section_origin = .;
    KEEP(*(.trap));
    *(.trap.*);
    *(.rwtext.interrupt .rwtext.interrupt.*)

    . = ALIGN(4);
    _rwtext_start = .;
    *(.rwtext.literal .rwtext .rwtext.literal.* .rwtext.*)
    *:esp_rom_spiflash.*(.literal .text .literal.* .text.*)

    *( .wifi0iram  .wifi0iram.*)
    *( .wifirxiram  .wifirxiram.*)
    *( .wifislprxiram  .wifislprxiram.*)
    *( .wifislpiram  .wifislpiram.*)
    *( .phyiram  .phyiram.*)
    *( .iram1  .iram1.*)
    *( .wifiextrairam.* )
    *( .coexiram.* )
    *( .high_perf_code_iram* )
    *( .coexsleepiram* )
    *( .wifiorslpiram* )
    *( .isr_iram* )
    *( .conn_iram* )
    *( .sleep_iram* )
    . = ALIGN(4);
    _rwtext_end = .;
  } > RWTEXT

  _rwtext_len = _rwtext_end - ORIGIN(RWTEXT);

  /* On chips where IRAM and DRAM alias the same SRAM, skip the DRAM view of .rwtext. */
  .data ALIGN(_rwtext_end - IRAM_DRAM_OFFSET, 16) : AT(LOADADDR(.rwtext) + SIZEOF(.rwtext))
  {
    _data_start = ABSOLUTE(.);
    *(.sdata .sdata.* .sdata2 .sdata2.*);
    *(.data .data.*);
    *(.data1)
    *( .dram1 .dram1.*)
    . = ALIGN(4);
    _data_end = ABSOLUTE(.);
  } > RWDATA

  .bss (NOLOAD) : ALIGN(4)
  {
    _bss_start = ABSOLUTE(.);
    *(.dynsbss)
    *(.sbss)
    *(.sbss.*)
    *(.gnu.linkonce.sb.*)
    *(.scommon)
    *(.sbss2)
    *(.sbss2.*)
    *(.gnu.linkonce.sb2.*)
    *(.dynbss)
    *(.sbss .sbss.* .bss .bss.*);
    *(.share.mem)
    *(.gnu.linkonce.b.*)
    *(COMMON)
    . = ALIGN(4);
    _bss_end = ABSOLUTE(.);
  } > RWDATA

  .noinit (NOLOAD) : ALIGN(4)
  {
    *(.noinit .noinit.*)
    *(.uninit .uninit.*)
    . = ALIGN(4);
    _noinit_end = .;
  } > RWDATA

  /* must be last section using RWDATA */
  .stack (NOLOAD) : ALIGN(4)
  {
    _stack_end = ABSOLUTE(.);
    _stack_end_cpu0 = ABSOLUTE(.);
    __stack_chk_guard = ABSOLUTE(_stack_end) + 60;

    . = ORIGIN(RWDATA) + LENGTH(RWDATA);
    . = ALIGN(4);
    _stack_start = ABSOLUTE(.);
    _stack_start_cpu0 = ABSOLUTE(.);
  } > RWDATA

  .dram2_uninit.bss (NOLOAD) : ALIGN(4)
  {
    _dram2_uninit_bss_start = ABSOLUTE(.);
    *(.dram2_uninit.bss)
    _dram2_uninit_bss_end = ABSOLUTE(.);
  } > dram2_seg

  .dram2_uninit (NOLOAD) : ALIGN(4)
  {
    *(.dram2_uninit)
  } > dram2_seg

  .espressif.metadata 0 (INFO) :
  {
    KEEP(*(.espressif.metadata));
  }

  .eh_frame 0 (INFO) :
  {
    KEEP(*(.eh_frame));
  }

  .got (INFO) :
  {
    KEEP(*(.got .got.*));
  }
}

_db_rwtext_src = DROM_BASE + LOADADDR(.rwtext);
_db_rwtext_dst = ADDR(.rwtext) - IRAM_DRAM_OFFSET;
_db_rwtext_end = _db_rwtext_dst + SIZEOF(.rwtext);

_db_data_src = DROM_BASE + LOADADDR(.data);
_db_data_dst = ADDR(.data);
_db_data_end = _db_data_dst + SIZEOF(.data);

/* Lowest DRAM address not shadowed by .rwtext; used by esp-riscv-rt's stack overflow check. */
_dram_data_start = _rwtext_end - IRAM_DRAM_OFFSET;

ASSERT(_stack_start - _stack_end >= 8192, "Main stack is smaller than 8192 bytes.");
ASSERT(SIZEOF(.got) == 0, "Dynamic relocations (.got) are not supported.");
ASSERT(_direct_boot_entry == IROM_BASE + DIRECT_BOOT_MAGIC_OFFSET + 8,
  "The direct-boot entry stub must directly follow the magic words.");
