#!/usr/bin/env python3
"""build/ramfs.py — monta a árvore da raiz volátil (estilo Linux/FHS).

A árvore `ramfs/` da raiz deste repositório é o esqueleto do sistema; ela é
entregue ao submódulo DNU (`RAMFS_DIR`) e entra no initramfs **dentro** do
dzImage, ao lado do kernel. Esta etapa:

  * cria os diretórios do esqueleto (`ramfs/`);
  * compila o userland (build/userland.py);
  * compila os componentes dos submódulos (build/components.py);
  * copia os binários DEF para os destinos (/bin, /sbin, /bin/utilities);
  * copia a fonte PSF do DNU, a libc e o linker script para /usr;
  * empacota o tema .tpf (se existir) e garante system/install/.
"""
from __future__ import annotations

import shutil
import subprocess
from pathlib import Path

from .common import (
    COMMON_DIR, DNU_DIR, LINKER_LD, RAMFS_DIRS, RAMFS_ROOT, RAMFS_TREE, ROOT,
    USER_DIR, USER_LIBC_DIR, fail, run, tree_newer,
)
from . import userland, components


def ensure_skeleton() -> None:
    """Cria os diretórios base do esqueleto ramfs/ (se não existirem)."""
    for directory in RAMFS_DIRS:
        (RAMFS_ROOT / directory).mkdir(parents=True, exist_ok=True)


def copy_userland(tree: Path) -> None:
    """Copia os binários DEF compilados para os destinos no ramfs."""
    results = userland.build_all()
    for name, path in results["programs"].items():
        destination = tree / ("sbin" if name in ("init", "getty") else "bin") / name
        shutil.copy2(path, destination)
    for utility in results["utilities"]:
        (tree / "bin").mkdir(parents=True, exist_ok=True)
        shutil.copy2(utility, tree / "bin" / utility.name)
    for utility in results["c_utilities"]:
        shutil.copy2(utility, tree / "bin" / utility.name)


def copy_components(tree: Path) -> None:
    """Copia os binários DEF dos componentes (submódulos) para o ramfs."""
    for name, path in components.build_all().items():
        if path is not None:
            shutil.copy2(path, tree / "bin" / name)


def copy_system_files(tree: Path) -> None:
    """Copia fonte PSF, libc estática, objetos crt e o linker script."""
    font_src = DNU_DIR / "src" / "assets" / "zap-vga16.psf"
    if not font_src.exists():
        fail(f"fonte PSF do kernel ausente: {font_src}")
    (tree / "system" / "fonts").mkdir(parents=True, exist_ok=True)
    shutil.copy2(font_src, tree / "system" / "fonts" / "zap-vga16.psf")

    include_dir = tree / "usr" / "include"
    (tree / "usr" / "lib").mkdir(parents=True, exist_ok=True)
    include_dir.mkdir(parents=True, exist_ok=True)
    shutil.copytree(USER_LIBC_DIR / "include", include_dir, dirs_exist_ok=True)
    static_lib = USER_DIR / "libdreamcore.a"
    if static_lib.exists():
        shutil.copy2(static_lib, tree / "usr" / "lib" / "libdreamcore.a")
        shutil.copy2(static_lib, tree / "usr" / "lib" / "libc.a")
    # O linker script do userspace fica disponível na raiz volátil (o
    # toolchain instalado na máquina ImagineOS o usa para reempacotar apps).
    shutil.copy2(LINKER_LD, tree / "usr" / "lib" / "imagineos.ld")

    # Payload do instalador: o próprio dzImage e os arquivos do Limine chegam
    # da ISO (build/iso.py); aqui só garantimos o diretório.
    (tree / "system" / "install").mkdir(parents=True, exist_ok=True)


def package_theme(tree: Path) -> None:
    """Empacota o tema clássico (.tpf — TAR ustar com themeConfig.json + CSS)."""
    theme_dir = RAMFS_ROOT / "system" / "themes" / "classic"
    config = theme_dir / "themeConfig.json"
    (tree / "system" / "themes").mkdir(parents=True, exist_ok=True)
    if not config.exists():
        print("temas: fontes em ramfs/system/themes/classic ausentes; WM usará o tema embutido")
        return
    target = tree / "system" / "themes" / "classic.tpf"
    run(["tar", "--format=ustar", "-C", str(theme_dir),
         "--exclude=*cursor*", "-cf", str(target), "themeConfig.json", "window"])
    shutil.copy2(target, tree / "system" / "themes" / "current.tpf")


def build_tree() -> Path:
    """Monta a árvore completa que o DNU empacota no initramfs do dzImage."""
    ensure_skeleton()
    if RAMFS_TREE.exists():
        shutil.rmtree(RAMFS_TREE)
    shutil.copytree(RAMFS_ROOT, RAMFS_TREE, symlinks=True)
    copy_userland(RAMFS_TREE)
    copy_components(RAMFS_TREE)
    copy_system_files(RAMFS_TREE)
    package_theme(RAMFS_TREE)
    # Checagem essencial: o dzImage só faz sentido com o init presente.
    # Se não houver init, o sistema inicia direto na GUI (kernel faz loop em kernel_entry).
    if not (RAMFS_TREE / "sbin" / "init").exists():
        print("aviso: sbin/init não foi gerado — sistema iniciará direto na GUI (kernel loop)")
    return RAMFS_TREE


def main() -> int:
    try:
        tree = build_tree()
        print(f"árvore do ramfs pronta: {tree}")
    except SystemExit as e:
        return int(e.code or 1)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
