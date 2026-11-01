//! isobuilder — monta a ISO inicializável do ImagineOS a partir dos payloads.
//!
//! É a ferramenta de host (roda no Linux, não no ImagineOS) que `build/iso.py`
//! chama para produzir a imagem El Torito UEFI:
//!
//!   /boot/dzImage          DNU Zipped Image — kernel + initramfs num arquivo
//!   /boot/BOOTX64.EFI      Limine (bootloader EFI, na imagem FAT do ESP)
//!   /limine.conf           configuração do Limine
//!   /efi.img               imagem FAT registrada como El Torito EFI
//!
//! xorriso produz ISO 9660 com Rock Ridge (-R), Joliet (-J) e El Torito
//! (--efi-boot-part), exatamente o combo que distribuições Linux usam. A
//! imagem FAT do ESP é dimensionada pelo tamanho dos payloads (BOOTX64.EFI +
//! dzImage) com folga, em vez de um tamanho fixo.
//!
//! Uso:
//!   isobuilder --dzimage dzImage --bootloader BOOTX64.EFI \
//!       --config limine.conf --output dreamcore-...-astrid.iso
//!
//! Requisitos no host: xorriso, mkfs.vfat (dosfstools), mtools (mmd/mcopy),
//! dd (coreutils). A ferramenta detecta e explica o que está faltando, com
//! código de saída apropriado (2 = dependência ausente ou erro de build).
use std::path::{Path, PathBuf};
use std::process::Command;

const VOLUME_ID: &str = "DREAMCORE";

struct Args {
    dzimage: PathBuf,
    bootloader: PathBuf,
    config: PathBuf,
    output: PathBuf,
    staging: PathBuf,
    kernel: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut dzimage = None;
    let mut bootloader = None;
    let mut config = None;
    let mut output = None;
    let mut staging = None;
    let mut kernel = None;
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        let mut take = |slot: &mut Option<PathBuf>, name: &str| -> Result<(), String> {
            let value = iter
                .next()
                .ok_or_else(|| format!("opção --{name} exige um valor"))?;
            *slot = Some(PathBuf::from(value));
            Ok(())
        };
        match arg.as_str() {
            "--dzimage" => take(&mut dzimage, "dzimage")?,
            "--bootloader" => take(&mut bootloader, "bootloader")?,
            "--config" => take(&mut config, "config")?,
            "--output" => take(&mut output, "output")?,
            "--staging" => take(&mut staging, "staging")?,
            "--kernel" => take(&mut kernel, "kernel")?,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("argumento desconhecido: {other} (veja --help)")),
        }
    }
    Ok(Args {
        dzimage: dzimage.ok_or("opção obrigatória ausente: --dzimage")?,
        bootloader: bootloader.ok_or("opção obrigatória ausente: --bootloader")?,
        config: config.ok_or("opção obrigatória ausente: --config")?,
        output: output.ok_or("opção obrigatória ausente: --output")?,
        staging: staging.unwrap_or_else(|| {
            let mut tmp = std::env::temp_dir();
            tmp.push("imagineos-iso-staging");
            tmp
        }),
        kernel,
    })
}

fn print_usage() {
    println!(
        "uso: isobuilder --dzimage <dzImage> --bootloader <BOOTX64.EFI> --config <limine.conf> --output <arquivo.iso> [--staging <dir>]"
    );
}

fn check_inputs(args: &Args) -> Result<(), String> {
    for (role, path) in [
        ("dzImage", &args.dzimage),
        ("bootloader", &args.bootloader),
        ("config", &args.config),
    ] {
        if !path.is_file() {
            return Err(format!("{role} não encontrado: {}", path.display()));
        }
    }
    Ok(())
}

fn require_tool(name: &str, hint: &str) -> Result<PathBuf, String> {
    match which(name) {
        Some(path) => Ok(path),
        None => Err(format!(
            "ferramenta ausente no host: {name}\n  instale com: {hint}"
        )),
    }
}

/// Implementação mínima de `which` sem dependências externas.
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn run_checked(tool: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(tool)
        .args(args)
        .status()
        .map_err(|error| format!("falha ao executar {tool}: {error}"))?;
    if !status.success() {
        return Err(format!("comando falhou: {tool} {}", args.join(" ")));
    }
    Ok(())
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("isobuilder: {message}");
            print_usage();
            std::process::exit(2);
        }
    };
    if let Err(error) = build_iso(&args) {
        eprintln!("isobuilder: {error}");
        std::process::exit(2);
    }
    println!(
        "ISO gerada com sucesso: {} ({} bytes)",
        args.output.display(),
        std::fs::metadata(&args.output).map(|m| m.len()).unwrap_or(0)
    );
}

fn build_iso(args: &Args) -> Result<(), String> {
    check_inputs(args)?;
    let xorriso = require_tool("xorriso", "apt install xorriso")?;
    let mkfs_vfat = require_tool("mkfs.vfat", "apt install dosfstools")?;
    let mmd = require_tool("mmd", "apt install mtools")?;
    let mcopy = require_tool("mcopy", "apt install mtools")?;
    let dd = require_tool("dd", "instale coreutils (apt install coreutils)")?;

    // Limpa o staging e cria o layout de uma live CD Linux.
    if args.staging.exists() {
        std::fs::remove_dir_all(&args.staging)
            .map_err(|e| format!("não foi possível limpar o staging: {e}"))?;
    }
    let boot_dir = args.staging.join("boot");
    let efi_dir = args.staging.join("EFI/BOOT");
    std::fs::create_dir_all(&boot_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&efi_dir).map_err(|e| e.to_string())?;

    std::fs::copy(&args.dzimage, boot_dir.join("dzImage")).map_err(|e| e.to_string())?;
    std::fs::copy(&args.bootloader, efi_dir.join("BOOTX64.EFI")).map_err(|e| e.to_string())?;
    std::fs::copy(&args.config, args.staging.join("limine.conf")).map_err(|e| e.to_string())?;
    // O dzImage é o único artefato publicado pelo DNU: o kernel ELF (membro
    // kind=kernel) é extraído dele e vai na ISO como /boot/kernel.elf, porque
    // é esse ELF que o Limine carrega e executa (o dzImage completo entra como
    // módulo, e a tabela de membros dele aponta para o mesmo kernel).
    let kernel = match &args.kernel {
        Some(path) => path.clone(),
        None => {
            let kernel_path = args.staging.join("kernel.elf");
            run_checked(
                "dfs-image",
                &[
                    "--dump-kernel",
                    args.dzimage.to_str().ok_or("caminho não UTF-8")?,
                    kernel_path.to_str().ok_or("caminho não UTF-8")?,
                ],
            )?;
            kernel_path
        }
    };
    std::fs::copy(&kernel, boot_dir.join("kernel.elf")).map_err(|e| e.to_string())?;

    // Dimensiona a imagem FAT do ESP com folga para os payloads de boot
    // (BOOTX64.EFI + dzImage + kernel.elf), em vez de um tamanho fixo.
    let payload_bytes = file_size(&args.bootloader)
        + file_size(&args.dzimage)
        + file_size(&kernel);
    let mib = 1024 * 1024;
    let blocks = (payload_bytes + mib - 1) / mib + 16;
    let efi_img = args.staging.join("efi.img");
    let efi_img_str = efi_img.to_str().ok_or("caminho do ESP não é UTF-8")?;
    run_checked(
        "dd",
        &[
            "if=/dev/zero",
            &format!("of={efi_img_str}"),
            "bs=1M",
            &format!("count={blocks}"),
        ],
    )?;
    run_checked("mkfs.vfat", &[efi_img_str])?;
    run_checked("mmd", &["-i", efi_img_str, "::/EFI", "::/EFI/BOOT", "::/boot"])?;
    run_checked(
        "mcopy",
        &[
            "-i", efi_img_str,
            efi_dir.join("BOOTX64.EFI").to_str().ok_or("caminho não UTF-8")?,
            "::/EFI/BOOT/",
        ],
    )?;
    run_checked(
        "mcopy",
        &[
            "-i", efi_img_str,
            args.staging.join("limine.conf").to_str().ok_or("caminho não UTF-8")?,
            "::/limine.conf",
        ],
    )?;
    run_checked(
        "mcopy",
        &[
            "-i", efi_img_str,
            boot_dir.join("dzImage").to_str().ok_or("caminho não UTF-8")?,
            "::/boot/",
        ],
    )?;
    run_checked(
        "mcopy",
        &[
            "-i", efi_img_str,
            boot_dir.join("kernel.elf").to_str().ok_or("caminho não UTF-8")?,
            "::/boot/",
        ],
    )?;

    // xorriso produz ISO 9660 com Rock Ridge (-R), Joliet (-J) e El Torito
    // (--efi-boot-part), exatamente o combo que distribuições Linux usam.
    let staging = args.staging.to_str().ok_or("caminho do staging não é UTF-8")?;
    run_checked(
        "xorriso",
        &[
            "-as", "mkisofs",
            "-R", "-J",
            "-V", VOLUME_ID,
            "--efi-boot", "efi.img",
            "-efi-boot-part", "--efi-boot-image",
            "--protective-msdos-label",
            staging,
            "-o",
            args.output.to_str().ok_or("caminho de saída não é UTF-8")?,
        ],
    )?;
    let _ = std::fs::remove_dir_all(&args.staging);
    Ok(())
}

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}
