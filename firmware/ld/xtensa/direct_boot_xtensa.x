/*
 * Direct-boot layout for the ESP32-S3.
 *
 * Expects the chip script to define the regions (vectors_seg, iram_seg, dram_seg, dram2_seg,
 * dcache_reclaimed_seg, irom_seg, drom_seg, rtc_fast_seg, rtc_slow_seg) and
 * DIRECT_BOOT_MAGIC_OFFSET, IROM_BASE, DROM_BASE and IRAM_DRAM_OFFSET (see the RISC-V variant).
 *
 * Flash layout (LMA == flash offset, VMA == IROM_BASE/DROM_BASE + LMA):
 *   [0xFF padding] | header | .text | .rodata (incl. copy table) | .vectors | .rwtext | .data
 *   | .rtc_fast.text | .rtc_fast.data | .rtc_slow.text | .rtc_slow.data
 */

/* GNU ld uses the last ENTRY, so this overrides `ENTRY(Reset)` from hal-defaults.x */
ENTRY(_direct_boot_entry)

/* From xtensa-lx-rt's exception.x, which can't be included because it places .vectors without
   a load address. */
PROVIDE(__exception = __default_exception);
PROVIDE(__user_exception = __default_user_exception);
PROVIDE(__double_exception = __default_double_exception);
PROVIDE(__level_1_interrupt = __default_interrupt);
PROVIDE(__level_2_interrupt = __default_interrupt);
PROVIDE(__level_3_interrupt = __default_interrupt);
PROVIDE(__level_4_interrupt = __default_interrupt);
PROVIDE(__level_5_interrupt = __default_interrupt);
PROVIDE(__level_6_interrupt = __default_interrupt);
PROVIDE(__level_7_interrupt = __default_interrupt);

PROVIDE(Timer0 = __default_user_exception);
PROVIDE(Timer1 = __default_user_exception);
PROVIDE(Timer2 = __default_user_exception);
PROVIDE(Timer3 = __default_user_exception);
PROVIDE(Profiling = __default_user_exception);
PROVIDE(NMI = __default_user_exception);
PROVIDE(Software0 = __default_user_exception);
PROVIDE(Software1 = __default_user_exception);

PROVIDE(__naked_user_exception = __default_naked_exception);
PROVIDE(__naked_kernel_exception = __default_naked_exception);
PROVIDE(__naked_double_exception = __default_naked_double_exception);
PROVIDE(__naked_level_2_interrupt = __default_naked_level_2_interrupt);
PROVIDE(__naked_level_3_interrupt = __default_naked_level_3_interrupt);
PROVIDE(__naked_level_4_interrupt = __default_naked_level_4_interrupt);
PROVIDE(__naked_level_5_interrupt = __default_naked_level_5_interrupt);
PROVIDE(__naked_level_6_interrupt = __default_naked_level_6_interrupt);
PROVIDE(__naked_level_7_interrupt = __default_naked_level_7_interrupt);

EXTERN(__default_exception);
EXTERN(__default_double_exception);
EXTERN(__default_interrupt);

EXTERN(__default_naked_exception);
EXTERN(__default_naked_double_exception);
EXTERN(__default_naked_level_2_interrupt);
EXTERN(__default_naked_level_3_interrupt);
EXTERN(__default_naked_level_4_interrupt);
EXTERN(__default_naked_level_5_interrupt);
EXTERN(__default_naked_level_6_interrupt);
EXTERN(__default_naked_level_7_interrupt);

SECTIONS {
  .direct_boot.header IROM_BASE + DIRECT_BOOT_MAGIC_OFFSET : AT(DIRECT_BOOT_MAGIC_OFFSET)
  {
    LONG(0xaedb041d)
    LONG(0xaedb041d)
  } > ROTEXT

  .text IROM_BASE + DIRECT_BOOT_MAGIC_OFFSET + 8 : AT(DIRECT_BOOT_MAGIC_OFFSET + 8)
  {
    _stext = .;
    _text_start = ABSOLUTE(.);
    /* Must be first: the ROM jumps to the instruction right after the magic. */
    KEEP(*(.direct_boot.entry));

    /* global_asm! literal pools and their consumers, see esp-hal's text.x */
    *(.literal)
    *(.text.__post_init)
    *(.text.__pre_init)
    *(.literal.* .text .text.*)
    . = ALIGN(4);
    _text_end = ABSOLUTE(.);
    _etext = .;
  } > ROTEXT

  .rodata ALIGN(DROM_BASE + LOADADDR(.text) + SIZEOF(.text), 16) : AT(ADDR(.rodata) - DROM_BASE)
  {
    _rodata_start = ABSOLUTE(.);

    /* (source in flash, destination, destination end) word triples */
    _direct_boot_copy_table_start = ABSOLUTE(.);
    LONG(_db_vectors_src) LONG(_db_vectors_dst) LONG(_db_vectors_end)
    LONG(_db_rwtext_src) LONG(_db_rwtext_dst) LONG(_db_rwtext_end)
    LONG(_db_data_src) LONG(_db_data_dst) LONG(_db_data_end)
    LONG(_db_rtc_fast_text_src) LONG(_db_rtc_fast_text_dst) LONG(_db_rtc_fast_text_end)
    LONG(_db_rtc_fast_data_src) LONG(_db_rtc_fast_data_dst) LONG(_db_rtc_fast_data_end)
    LONG(_db_rtc_slow_text_src) LONG(_db_rtc_slow_text_dst) LONG(_db_rtc_slow_text_end)
    LONG(_db_rtc_slow_data_src) LONG(_db_rtc_slow_data_dst) LONG(_db_rtc_slow_data_end)
    _direct_boot_copy_table_end = ABSOLUTE(.);

    *(.rodata .rodata.*)
    *(.rodata_wlog_*.*)
    . = ALIGN(4);
    _rodata_end = ABSOLUTE(.);
  } > RODATA

  /* Each Xtensa vector has a fixed 64 byte slot, see xtensa-lx-rt's exception.x */
  .vectors ORIGIN(vectors_seg) : AT(LOADADDR(.rodata) + SIZEOF(.rodata)) SUBALIGN(0x40)
  {
    _init_start = ABSOLUTE(.);
    KEEP(*(.WindowOverflow4.text));
    KEEP(*(.WindowUnderflow4.text));
    KEEP(*(.WindowOverflow8.text));
    KEEP(*(.WindowUnderflow8.text));
    KEEP(*(.WindowOverflow12.text));
    KEEP(*(.WindowUnderflow12.text));
    KEEP(*(.Level2InterruptVector.text));
    KEEP(*(.Level3InterruptVector.text));
    KEEP(*(.Level4InterruptVector.text));
    KEEP(*(.Level5InterruptVector.text));
    KEEP(*(.DebugExceptionVector.text));
    KEEP(*(.NMIExceptionVector.text));
    KEEP(*(.KernelExceptionVector.text));
    KEEP(*(.UserExceptionVector.text));
    . = . + 0x40;
    KEEP(*(.DoubleExceptionVector.text));
    . = ALIGN(0x400);
    _init_end = ABSOLUTE(.);
  } > vectors_seg

  /* xtensa-lx-rt's `Reset` lives here, so it must be copied before jumping to it. */
  .rwtext ORIGIN(iram_seg) : AT(LOADADDR(.vectors) + SIZEOF(.vectors))
  {
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
  } > iram_seg

  _rwtext_len = _rwtext_end - ORIGIN(iram_seg);

  /* skip the DRAM view of .vectors and .rwtext */
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

  .rtc_fast.text ORIGIN(rtc_fast_seg) : AT(LOADADDR(.data) + SIZEOF(.data))
  {
    . = ALIGN(4);
    *(.rtc_fast.literal .rtc_fast.text .rtc_fast.literal.* .rtc_fast.text.*)
    . = ALIGN(4);
  } > rtc_fast_seg

  .rtc_fast.data : AT(LOADADDR(.rtc_fast.text) + SIZEOF(.rtc_fast.text))
  {
    . = ALIGN(4);
    _rtc_fast_data_start = ABSOLUTE(.);
    *(.rtc_fast.data .rtc_fast.data.*)
    . = ALIGN(4);
    _rtc_fast_data_end = ABSOLUTE(.);
  } > rtc_fast_seg

  .rtc_fast.bss (NOLOAD) :
  {
    . = ALIGN(4);
    _rtc_fast_bss_start = ABSOLUTE(.);
    *(.rtc_fast.bss .rtc_fast.bss.*)
    . = ALIGN(4);
    _rtc_fast_bss_end = ABSOLUTE(.);
  } > rtc_fast_seg

  .rtc_fast.persistent (NOLOAD) :
  {
    . = ALIGN(4);
    _rtc_fast_persistent_start = ABSOLUTE(.);
    *(.rtc_fast.persistent .rtc_fast.persistent.*)
    . = ALIGN(4);
    _rtc_fast_persistent_end = ABSOLUTE(.);
  } > rtc_fast_seg

  .rtc_slow.text ORIGIN(rtc_slow_seg) : AT(LOADADDR(.rtc_fast.data) + SIZEOF(.rtc_fast.data))
  {
    . = ALIGN(4);
    *(.rtc_slow.literal .rtc_slow.text .rtc_slow.literal.* .rtc_slow.text.*)
    . = ALIGN(4);
  } > rtc_slow_seg

  .rtc_slow.data : AT(LOADADDR(.rtc_slow.text) + SIZEOF(.rtc_slow.text))
  {
    . = ALIGN(4);
    _rtc_slow_data_start = ABSOLUTE(.);
    *(.rtc_slow.data .rtc_slow.data.*)
    . = ALIGN(4);
    _rtc_slow_data_end = ABSOLUTE(.);
  } > rtc_slow_seg

  .rtc_slow.bss (NOLOAD) :
  {
    . = ALIGN(4);
    _rtc_slow_bss_start = ABSOLUTE(.);
    *(.rtc_slow.bss .rtc_slow.bss.*)
    . = ALIGN(4);
    _rtc_slow_bss_end = ABSOLUTE(.);
  } > rtc_slow_seg

  .rtc_slow.persistent (NOLOAD) :
  {
    . = ALIGN(4);
    _rtc_slow_persistent_start = ABSOLUTE(.);
    *(.rtc_slow.persistent .rtc_slow.persistent.*)
    . = ALIGN(4);
    _rtc_slow_persistent_end = ABSOLUTE(.);
  } > rtc_slow_seg

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

  .dcache_reclaimed_uninit (NOLOAD) : ALIGN(4)
  {
    *(.dcache_reclaimed_uninit)
  } > dcache_reclaimed_seg

  .espressif.metadata 0 (INFO) :
  {
    KEEP(*(.espressif.metadata));
  }

  .eh_frame 0 (INFO) :
  {
    KEEP(*(.eh_frame));
  }
}

/* xtensa-lx-rt would copy .data from here, but esp-hal's `__init_data` disables that and the
   entry stub has already done it. */
_sidata = DROM_BASE + LOADADDR(.data);

_db_vectors_src = DROM_BASE + LOADADDR(.vectors);
_db_vectors_dst = ADDR(.vectors) - IRAM_DRAM_OFFSET;
_db_vectors_end = _db_vectors_dst + SIZEOF(.vectors);

_db_rwtext_src = DROM_BASE + LOADADDR(.rwtext);
_db_rwtext_dst = ADDR(.rwtext) - IRAM_DRAM_OFFSET;
_db_rwtext_end = _db_rwtext_dst + SIZEOF(.rwtext);

_db_data_src = DROM_BASE + LOADADDR(.data);
_db_data_dst = ADDR(.data);
_db_data_end = _db_data_dst + SIZEOF(.data);

_db_rtc_fast_text_src = DROM_BASE + LOADADDR(.rtc_fast.text);
_db_rtc_fast_text_dst = ADDR(.rtc_fast.text);
_db_rtc_fast_text_end = _db_rtc_fast_text_dst + SIZEOF(.rtc_fast.text);

_db_rtc_fast_data_src = DROM_BASE + LOADADDR(.rtc_fast.data);
_db_rtc_fast_data_dst = ADDR(.rtc_fast.data);
_db_rtc_fast_data_end = _db_rtc_fast_data_dst + SIZEOF(.rtc_fast.data);

_db_rtc_slow_text_src = DROM_BASE + LOADADDR(.rtc_slow.text);
_db_rtc_slow_text_dst = ADDR(.rtc_slow.text);
_db_rtc_slow_text_end = _db_rtc_slow_text_dst + SIZEOF(.rtc_slow.text);

_db_rtc_slow_data_src = DROM_BASE + LOADADDR(.rtc_slow.data);
_db_rtc_slow_data_dst = ADDR(.rtc_slow.data);
_db_rtc_slow_data_end = _db_rtc_slow_data_dst + SIZEOF(.rtc_slow.data);

ASSERT(SIZEOF(.vectors) == 0x400, ".vectors must be exactly 0x400 bytes - a vector overflowed its 64-byte slot");
ASSERT(_stack_start - _stack_end >= 8192, "Main stack is smaller than 8192 bytes.");
ASSERT(_direct_boot_entry == IROM_BASE + DIRECT_BOOT_MAGIC_OFFSET + 8,
  "The direct-boot entry stub must directly follow the magic words.");
