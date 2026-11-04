#!/usr/bin/env python3
"""build/iso.py — monta a ISO inicializável com o isobuilder (imagine-tools).

Orquestra as etapas anteriores (kernel, ramfs, dzImage) e chama a
ferramenta `isobuilder` do submódulo `imagine-tools`, que produz a imagem
El Torito UEFI com o dzImage + Limine.

O dzImage contém APENAS o kernel ELF. O initrd e o filesystem.tar são
arquivos separados incluídos na ISO.
"""
from __future__ import annotations

import os
import shutil
from pathlib import Path

from . import kernel, ramfs
from .common import (
    BOOT_DIR, BUILD_DIR, DNU_DIR, DZ_IMAGE, DZIMAGE_BIN, ISO_DIR, ISO_IMAGE, LIMINE_DIR, ROOT, TARGET,
    cargo, fail, require_tool, run, tree_newer,
)

ISO_TOOL = ROOT / "imagine-tools" / "isobuilder"
ISO_TOOL_BIN = ISO_TOOL / "target" / "x86_64-unknown-linux-gnu" / "release" / "isobuilder"


def build_isobuilder() -> Path:
    """Compila o isobuilder (ferramenta de host do submódulo imagine-tools)."""
    if not ISO_TOOL.exists():
        fail(
            f"submódulo imagine-tools ausente em {ISO_TOOL}.\n"
            "  inicialize com: git submodule update --init imagine-tools"
        )
    if not ISO_TOOL_BIN.exists():
        cargo(False, "build", "--release", cwd=ISO_TOOL)
    if not ISO_TOOL_BIN.exists():
        fail(f"isobuilder não foi gerado: {ISO_TOOL_BIN}")
    return ISO_TOOL_BIN


def build_iso() -> Path:
    """Compila tudo e monta a ISO final; devolve o caminho do arquivo."""
    require_tool("xorriso", "apt install xorriso")
    require_tool("mkfs.vfat", "apt install dosfstools")
    require_tool("mmd", "apt install mtools")
    require_tool("mcopy", "apt install mtools")

    # Primeiro compila o kernel e as ferramentas de host (dnu-pack, etc.)
    # porque o userland precisa do dnu-pack para empacotar os programas DEF.
    kernel.build_kernel()
    kernel.build_tools()

    # Depois monta a árvore do ramfs (que compila o userland com dnu-pack disponível)
    tree = ramfs.build_tree()

    # Finalmente empacota o kernel no dzImage (apenas o kernel ELF, sem initramfs).
    # O initramfs é gerado como arquivo separado (initrd).
    dzimage = kernel.build_dzimage(tree)
    isobuilder = build_isobuilder()

    bootloader = LIMINE_DIR / "BOOTX64.EFI"
    if not bootloader.exists():
        fail(
            f"bootloader UEFI do Limine ausente: {bootloader}\n"
            "  o submódulo imagine-bootloader deve conter BOOTX64.EFI "
            "(baixe uma release do Limine e copie o arquivo para lá)"
        )
    config = LIMINE_DIR / "limine.conf"
    if not config.exists():
        fail(f"configuração do bootloader ausente: {config}")
    print(f"[iso] Usando limine.conf: {config}", file=__import__('sys').stderr)

    # O Limine carrega o ELF do kernel (gerado pelo build_kernel); o
    # dzImage contém apenas o kernel ELF. O initrd e filesystem.tar são
    # arquivos separados incluídos na ISO.
    ISO_DIR.mkdir(parents=True, exist_ok=True)
    kernel_elf = ISO_DIR / "kernel.elf"
    kernel_path = DNU_DIR / "target" / TARGET / "release" / "imagineos-dnu-kernel"
    if not kernel_path.exists():
        fail(f"kernel ELF não encontrado: {kernel_path}")
    shutil.copy2(kernel_path, kernel_elf)

    # Copia o initrd e o filesystem.tar para o diretório ISO
    initrd_src = BUILD_DIR / "initrd"
    filesystem_tar_src = BUILD_DIR / "filesystem.tar"

    if not initrd_src.exists():
        fail(f"initrd não encontrado: {initrd_src}")
    if not filesystem_tar_src.exists():
        fail(f"filesystem.tar não encontrado: {filesystem_tar_src}")

    initrd_dst = ISO_DIR / "initrd"
    filesystem_tar_dst = ISO_DIR / "filesystem.tar"
    shutil.copy2(initrd_src, initrd_dst)
    shutil.copy2(filesystem_tar_src, filesystem_tar_dst)

    print(f"[iso] dzImage: {DZ_IMAGE} ({DZ_IMAGE.stat().st_size} bytes)")
    print(f"[iso] initrd:  {initrd_dst} ({initrd_dst.stat().st_size} bytes)")
    print(f"[iso] filesystem.tar: {filesystem_tar_dst} ({filesystem_tar_dst.stat().st_size} bytes)")

    ISO_IMAGE.parent.mkdir(parents=True, exist_ok=True)
    env = {"PATH": f"{kernel.DNU_BUILD}/tools:{os.environ.get('PATH', '')}"}
    run([str(isobuilder),
         "--dzimage", str(dzimage),
         "--kernel", str(kernel_elf),
         "--initrd", str(initrd_dst),
         "--filesystem-tar", str(filesystem_tar_dst),
         "--bootloader", str(bootloader),
         "--config", str(config),
         "--output", str(ISO_IMAGE)],
        env=env)
    print(f"ISO inicializável pronta: {ISO_IMAGE}")
    return ISO_IMAGE


def main() -> int:
    try:
        build_iso()
    except SystemExit as e:
        return int(e.code or 1)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())