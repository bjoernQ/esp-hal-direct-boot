use std::{env, error::Error, fs, path::PathBuf};

const CHIPS: &[&str] = &[
    "esp32c2", "esp32c3", "esp32c5", "esp32c6", "esp32c61", "esp32h2", "esp32h4", "esp32p4",
    "esp32s3", "esp32s31",
];

fn main() -> Result<(), Box<dyn Error>> {
    let enabled: Vec<&str> = CHIPS
        .iter()
        .copied()
        .filter(|chip| env::var_os(format!("CARGO_FEATURE_{}", chip.to_uppercase())).is_some())
        .collect();

    let chip = match enabled.as_slice() {
        [chip] => *chip,
        [] => return Err(format!("enable exactly one chip feature, one of: {CHIPS:?}").into()),
        _ => return Err(format!("only one chip feature may be enabled, got: {enabled:?}").into()),
    };

    let arch = if chip == "esp32s3" { "xtensa" } else { "riscv" };

    // The ROM of these chips leaves PMA entries enabled that the 2nd-stage bootloader replaces.
    println!("cargo::rustc-check-cfg=cfg(rom_pma)");
    if ["esp32c5", "esp32c61", "esp32p4", "esp32s31"].contains(&chip) {
        println!("cargo::rustc-cfg=rom_pma");
    }

    // esp-hal's own OUT_DIR (with `memory.x`, `linkall.x`, ...) is on the linker search path as
    // well, so every script copied here must have a name that doesn't clash with those.
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    copy(&format!("ld/{chip}/direct_boot.x"), &out)?;
    for entry in fs::read_dir(format!("ld/{arch}"))? {
        let path = entry?.path();
        copy(path.to_str().unwrap(), &out)?;
    }

    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=ld");

    Ok(())
}

fn copy(src: &str, out: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let name = std::path::Path::new(src).file_name().unwrap();
    fs::copy(src, out.join(name))?;
    println!("cargo:rerun-if-changed={src}");
    Ok(())
}
