//! Hello world booted directly by the ROM, without the ESP-IDF 2nd-stage bootloader.
//!
//! Besides printing, it checks the things the custom linker scripts and the entry stub are
//! responsible for, since nothing else loads RAM in direct-boot mode.

#![no_std]
#![no_main]
#![cfg_attr(target_arch = "xtensa", feature(asm_experimental_arch))]

use core::{
    ptr::{addr_of, addr_of_mut},
    sync::atomic::{AtomicBool, Ordering},
};

use esp_backtrace as _;
use esp_hal::{
    handler,
    interrupt::software::SoftwareInterrupt,
    main,
    peripherals::FROM_CPU_INTR2,
    ram,
    time::{Duration, Instant},
};
use esp_println::println;

mod direct_boot;

const DATA_PATTERN: u32 = 0xC0FF_EE42;

// Both are also written below, so the compiler can't turn them into constants.
static mut DATA_CHECK: u32 = DATA_PATTERN;
static mut BSS_CHECK: u32 = 0;

static SWINT_FIRED: AtomicBool = AtomicBool::new(false);

#[ram]
fn ram_function(x: u32) -> u32 {
    x.wrapping_mul(3).wrapping_add(1)
}

#[handler]
fn swint_handler() {
    // SAFETY: only used to clear the interrupt request that triggered this handler.
    unsafe { SoftwareInterrupt::<2>::new(FROM_CPU_INTR2::steal()) }.reset();
    SWINT_FIRED.store(true, Ordering::Release);
}

fn report(name: &str, ok: bool) {
    println!("[{}] {}", if ok { " OK " } else { "FAIL" }, name);
}

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    #[cfg(feature = "boot-trace")]
    direct_boot::mark(c"main reached, calling esp_hal::init");
    let peripherals = esp_hal::init(esp_hal::Config::default());
    #[cfg(feature = "boot-trace")]
    direct_boot::mark(c"esp_hal::init done");

    println!("Hello from direct boot!");

    // SAFETY: single-threaded, no references are created.
    let (data, bss) = unsafe {
        let data = addr_of!(DATA_CHECK).read_volatile();
        let bss = addr_of!(BSS_CHECK).read_volatile();
        addr_of_mut!(DATA_CHECK).write_volatile(data.wrapping_add(1));
        addr_of_mut!(BSS_CHECK).write_volatile(bss.wrapping_add(1));
        (data, bss)
    };
    println!(
        ".data @ {:p} = {:#010x}, .bss @ {:p} = {:#010x}",
        addr_of!(DATA_CHECK),
        data,
        addr_of!(BSS_CHECK),
        bss
    );
    report(".data copied from flash", data == DATA_PATTERN);
    report(".bss zeroed", bss == 0);

    let ram_fn: fn(u32) -> u32 = ram_function;
    println!("#[ram] function @ {:p}", ram_fn as *const ());
    report(".rwtext copied from flash", ram_fn(41) == 124);

    let mut swint = SoftwareInterrupt::new(peripherals.FROM_CPU_INTR2);
    swint.set_interrupt_handler(swint_handler);
    swint.raise();
    let start = Instant::now();
    while !SWINT_FIRED.load(Ordering::Acquire) && start.elapsed() < Duration::from_millis(100) {}
    report(
        "software interrupt handled (vectors in RAM)",
        SWINT_FIRED.load(Ordering::Acquire),
    );

    loop {
        println!("Bing!");

        let now = Instant::now();
        while now.elapsed() < Duration::from_millis(5000) {}
    }
}
