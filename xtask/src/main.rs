use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use object::{
    Endianness,
    elf::{FileHeader32, PT_LOAD, SHF_ALLOC, SHT_PROGBITS},
    read::elf::{FileHeader, ProgramHeader, SectionHeader},
};

const DIRECT_BOOT_MAGIC: [u8; 8] = [0x1d, 0x04, 0xdb, 0xae, 0x1d, 0x04, 0xdb, 0xae];
const BIN_NAME: &str = "direct-boot-hello";

#[derive(Parser)]
#[command(about = "Build and flash the direct-boot firmware")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build the firmware and convert it to a raw direct-boot image.
    Build {
        chip: Chip,
        #[command(flatten)]
        opts: BuildOpts,
    },
    /// Build, write the image to flash offset 0x0 and open the serial monitor.
    Run {
        chip: Chip,
        #[command(flatten)]
        opts: BuildOpts,
        /// Serial port of the target (default: let espflash pick).
        #[arg(short, long)]
        port: Option<String>,
    },
    /// Build the image for every supported chip.
    BuildAll {
        #[command(flatten)]
        opts: BuildOpts,
    },
}

#[derive(clap::Args, Clone, Copy)]
struct BuildOpts {
    /// Use the dev profile instead of release.
    #[arg(long)]
    debug: bool,
    /// Enable the firmware's `boot-trace` feature (early diagnostics via the ROM's printf).
    #[arg(long)]
    trace: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
enum Chip {
    Esp32c2,
    Esp32c3,
    Esp32c5,
    Esp32c6,
    Esp32c61,
    Esp32h2,
    Esp32h4,
    Esp32p4,
    Esp32s3,
    Esp32s31,
}

impl Chip {
    /// Name used for the firmware's cargo feature and espflash's `--chip`.
    fn name(self) -> &'static str {
        match self {
            Chip::Esp32c2 => "esp32c2",
            Chip::Esp32c3 => "esp32c3",
            Chip::Esp32c5 => "esp32c5",
            Chip::Esp32c6 => "esp32c6",
            Chip::Esp32c61 => "esp32c61",
            Chip::Esp32h2 => "esp32h2",
            Chip::Esp32h4 => "esp32h4",
            Chip::Esp32p4 => "esp32p4",
            Chip::Esp32s3 => "esp32s3",
            Chip::Esp32s31 => "esp32s31",
        }
    }

    fn target(self) -> &'static str {
        match self {
            Chip::Esp32c2 | Chip::Esp32c3 => "riscv32imc-unknown-none-elf",
            Chip::Esp32c5 | Chip::Esp32c6 | Chip::Esp32c61 | Chip::Esp32h2 => {
                "riscv32imac-unknown-none-elf"
            }
            Chip::Esp32h4 | Chip::Esp32p4 | Chip::Esp32s31 => "riscv32imafc-unknown-none-elf",
            Chip::Esp32s3 => "xtensa-esp32s3-none-elf",
        }
    }

    fn is_xtensa(self) -> bool {
        self == Chip::Esp32s3
    }

    /// Flash offset at which the ROM looks for the direct-boot magic.
    fn magic_offset(self) -> usize {
        if self == Chip::Esp32s3 { 0x400 } else { 0 }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    let firmware = root.join("firmware");

    match cli.command {
        Cmd::Build { chip, opts } => {
            build(&firmware, chip, opts)?;
        }
        Cmd::Run { chip, opts, port } => {
            let (elf, bin) = build(&firmware, chip, opts)?;
            flash_and_monitor(chip, &elf, &bin, port.as_deref())?;
        }
        Cmd::BuildAll { opts } => {
            let mut failed = Vec::new();
            for chip in Chip::value_variants() {
                if let Err(e) = build(&firmware, *chip, opts) {
                    eprintln!("error: {}: {e:#}", chip.name());
                    failed.push(chip.name());
                }
            }
            if !failed.is_empty() {
                bail!("failed to build: {}", failed.join(", "));
            }
            println!("all chips built successfully");
        }
    }

    Ok(())
}

/// Builds the firmware and returns the paths of the ELF and the flashable image.
fn build(firmware: &Path, chip: Chip, opts: BuildOpts) -> Result<(PathBuf, PathBuf)> {
    println!("=== building for {} ({})", chip.name(), chip.target());

    let mut cmd = Command::new("cargo");
    // Started via `cargo xtask`, so the environment contains the toolchain and package settings
    // of the xtask itself. Remove them so the firmware build picks its own.
    for (key, _) in std::env::vars_os() {
        let key = key.to_string_lossy();
        if key == "RUSTUP_TOOLCHAIN"
            || key == "RUSTC"
            || key == "RUSTDOC"
            || (key.starts_with("CARGO") && key != "CARGO_HOME")
        {
            cmd.env_remove(key.as_ref());
        }
    }
    if chip.is_xtensa() {
        cmd.arg("+esp");
    }
    let mut features = chip.name().to_string();
    if opts.trace {
        features.push_str(",boot-trace");
    }
    cmd.current_dir(firmware)
        .arg("build")
        .args(["--target", chip.target()])
        .args(["--features", &features]);
    if !opts.debug {
        cmd.arg("--release");
    }
    if chip.is_xtensa() {
        cmd.arg("-Zbuild-std=core,alloc");
    }

    let status = cmd.status().context("failed to run cargo")?;
    ensure!(status.success(), "cargo build failed for {}", chip.name());

    let profile = if opts.debug { "debug" } else { "release" };
    let target_dir = firmware.join("target");
    let built = target_dir.join(chip.target()).join(profile).join(BIN_NAME);

    // Several chips share a target triple, so keep a per-chip copy of the artifacts.
    let out_dir = target_dir.join("images").join(profile);
    fs::create_dir_all(&out_dir)?;
    let elf = out_dir.join(format!("{}.elf", chip.name()));
    let bin = out_dir.join(format!("{}.bin", chip.name()));
    fs::copy(&built, &elf).with_context(|| format!("failed to copy {}", built.display()))?;

    let image = elf_to_image(&elf, chip)?;
    fs::write(&bin, &image).with_context(|| format!("failed to write {}", bin.display()))?;
    println!("image: {} ({} bytes)", bin.display(), image.len());

    Ok((elf, bin))
}

/// Lays out every allocated section at its load address (== flash offset), like
/// `objcopy -O binary` but starting at flash offset 0 and filling gaps with 0xFF (erased flash).
fn elf_to_image(elf: &Path, chip: Chip) -> Result<Vec<u8>> {
    let data = fs::read(elf).with_context(|| format!("failed to read {}", elf.display()))?;
    let header = FileHeader32::<Endianness>::parse(&*data)?;
    let endian = header.endian()?;
    let segments = header.program_headers(endian, &*data)?;
    let sections = header.sections(endian, &*data)?;

    let mut chunks = Vec::new();
    for section in sections.iter() {
        let size = section.sh_size(endian) as u64;
        if section.sh_type(endian) != SHT_PROGBITS
            || section.sh_flags(endian) as u64 & SHF_ALLOC as u64 == 0
            || size == 0
        {
            continue;
        }

        let name = String::from_utf8_lossy(sections.section_name(endian, section)?).into_owned();
        let vma = section.sh_addr(endian) as u64;
        let segment = segments
            .iter()
            .find(|s| {
                let start = s.p_vaddr(endian) as u64;
                s.p_type(endian) == PT_LOAD
                    && s.p_filesz(endian) > 0
                    && start <= vma
                    && vma + size <= start + s.p_filesz(endian) as u64
            })
            .with_context(|| format!("no load segment contains section {name}"))?;
        let lma = segment.p_paddr(endian) as u64 + (vma - segment.p_vaddr(endian) as u64);

        println!("  {name:<20} vma {vma:#010x}  flash {lma:#08x}  size {size:#x}");
        chunks.push((lma as usize, section.data(endian, &*data)?));
    }

    let end = chunks.iter().map(|(lma, d)| lma + d.len()).max().unwrap_or(0);
    ensure!(end <= 0x40_0000, "image ends at {end:#x}, beyond the 4 MB the ROM maps");

    let mut image = vec![0xFF; end.next_multiple_of(4)];
    chunks.sort_by_key(|(lma, _)| *lma);
    for window in chunks.windows(2) {
        let (a, b) = (&window[0], &window[1]);
        ensure!(a.0 + a.1.len() <= b.0, "sections overlap in flash at {:#x}", b.0);
    }
    for (lma, bytes) in chunks {
        image[lma..lma + bytes.len()].copy_from_slice(bytes);
    }

    let offset = chip.magic_offset();
    ensure!(
        image.get(offset..offset + 8) == Some(&DIRECT_BOOT_MAGIC[..]),
        "direct-boot magic not found at flash offset {offset:#x}"
    );

    Ok(image)
}

fn flash_and_monitor(chip: Chip, elf: &Path, bin: &Path, port: Option<&str>) -> Result<()> {
    let mut cmd = Command::new("espflash");
    cmd.arg("write-bin").args(["--chip", chip.name()]);
    if let Some(port) = port {
        cmd.args(["--port", port]);
    }
    cmd.arg("--monitor").arg("--elf").arg(elf).arg("0x0").arg(bin);

    let status = cmd
        .status()
        .context("failed to run espflash - is it installed (`cargo install espflash`)?")?;
    ensure!(status.success(), "espflash failed");
    Ok(())
}
