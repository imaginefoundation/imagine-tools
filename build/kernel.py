#!/usr/bin/env python3
"""build/kernel.py — kernel e dzImage (delegados ao repositório DNU).

O kernel `dreamcore` e o formato DEF são propriedade do submódulo `dnu/`, que
publica um único artefato: `.build/dzImage` (apenas o kernel ELF, sem initramfs).
Este módulo chama o Makefile do DNU com `RAMFS_DIR` apontando para
a árvore montada por `build/ramfs.py` para gerar o initrd separado.

O submódulo dnu está no branch `main`, que contém o Makefile e as ferramentas
de host (dzimage, dnu-pack, dnulink, dfs-image) usados aqui e pelo userland.
"""
from __future__ import annotations

import shutil
import tarfile
from pathlib import Path

from .common import (
    BUILD_DIR, DNUDEBUG_BIN, DNUBOOT_BIN, DNUTEST_BIN, DNU_BUILD, DNU_DIR,
    DZ_IMAGE, DZIMAGE_BIN, RAMFS_TREE, TARGET, cargo, fail, run,
    require_nightly, require_rust_target, tree_newer,
)


def check_dnu() -> None:
    if not (DNU_DIR / "Makefile").exists():
        fail(
            f"submódulo DNU ausente em {DNU_DIR}.\n"
            "  inicialize com: git submodule update --init dnu"
        )
    if not (DNU_DIR / "src").exists():
        fail(
            f"checkout do submódulo DNU está vazio (branch errado?).\n"
            "  corrija com: cd dnu && git checkout main"
        )
    # O branch main do DNU contém o Makefile e as ferramentas de host; o
    # branch 6b2f765 (antigo pin) não tem Makefile nem tools/.
    if not (DNU_DIR / "tools").exists():
        fail(
            "submódulo DNU em branch sem Makefile/tools.\n"
            "  corrija com: cd dnu && git checkout main"
        )


def build_tools() -> None:
    """Compila as ferramentas de host do DNU (dzimage, dnudebug, dnutest, dnuboot)."""
    check_dnu()
    require_nightly()
    # As ferramentas só dependem do Makefile e das fontes do DNU; o cargo check
    # da ABI dentro do `make tools` já valida o bundle. Compilamos direto pelo
    # Makefile do DNU (ele sabe empacotar os bundles da ABI).
    targets = [DZIMAGE_BIN, DNUDEBUG_BIN, DNUTEST_BIN, DNUBOOT_BIN]
    if all(t.exists() for t in targets):
        return
    run(["make", "tools"], cwd=DNU_DIR)
    for tool in targets:
        if not tool.exists():
            fail(f"ferramenta de host não foi gerada: {tool}")


def build_kernel() -> Path:
    """Compila o kernel ELF (nightly + target x86_64-unknown-none)."""
    check_dnu()
    require_nightly()
    require_rust_target()
    kernel = DNU_DIR / "target" / TARGET / "release" / "imagineos-dnu-kernel"
    if kernel.exists():
        return kernel
    cargo(
        True, "build", "--release", "--target", TARGET,
        "--bin", "imagineos-dnu-kernel",
        "--config",
        'target.x86_64-unknown-none.rustflags=["-C","link-arg=-Tsrc/linker.ld","-C","relocation-model=static"]',
        cwd=DNU_DIR,
    )
    if not kernel.exists():
        fail("kernel ELF não foi gerado pelo cargo")
    return kernel


def build_dzimage(ramfs_tree: Path) -> Path:
    """Empacota o kernel ELF no dzImage (somente o kernel, sem initramfs).

    O initramfs é gerado como arquivo separado (initrd) e o sistema de
    arquivos completo é empacotado em filesystem.tar.
    """
    check_dnu()
    if not ramfs_tree.exists():
        fail(
            "árvore do ramfs ausente; rode a etapa de ramfs antes do dzImage.\n"
            "  execute: python3 build.py ramfs"
        )
    build_kernel()
    build_tools()
    # Verifica que o /sbin/init real (compilado pelo userland) está presente na
    # árvore do RAMFS. Não substituímos por um no-op: o init Rust em
    # imagine-common/init/init.rs é responsável por lançar ws_server, shell e
    # explorer.
    init_src = ramfs_tree / "sbin" / "init"
    if not init_src.exists():
        fail(
            f"/sbin/init não encontrado na árvore do RAMFS ({ramfs_tree}).\n"
            "  rode a etapa de userland antes do kernel: python3 build.py userland"
        )
    # O dzImage contém o kernel ELF mais o initramfs (quando RAMFS_DIR é
    # informado), para que o kernel possa montar o RAMFS no boot e executar
    # /sbin/init como PID 1.
    run(["make", "dzImage", f"RAMFS_DIR={ramfs_tree}"], cwd=DNU_DIR)
    dzimage = DNU_BUILD / "dzImage"
    if not dzimage.exists():
        fail("dzImage não foi gerado pelo DNU")
    BUILD_DIR.mkdir(parents=True, exist_ok=True)
    shutil.copy2(dzimage, DZ_IMAGE)
    run([str(DZIMAGE_BIN), "info", str(DZ_IMAGE)])

    # Agora gera o initrd (tar USTAR da árvore RAMFS completa).
    initrd_path = BUILD_DIR / "initrd"
    if initrd_path.exists():
        initrd_path.unlink()
    run([
        "tar", "--format=ustar", "--numeric-owner", "--owner=0", "--group=0",
        "-C", str(ramfs_tree), "-cf", str(initrd_path),
        ".",
    ])
    print(f"initrd gerado: {initrd_path} ({initrd_path.stat().st_size} bytes)")

    # Gera o filesystem.tar com o restante do sistema de arquivos
    # (tudo que está na árvore RAMFS, exceto o que já vai no initrd).
    # Como o initrd já contém tudo, criamos um tarball separado com
    # o conteúdo do sistema de arquivos para ser montado na raiz.
    filesystem_tar = BUILD_DIR / "filesystem.tar"
    if filesystem_tar.exists():
        filesystem_tar.unlink()
    run([
        "tar", "--format=ustar", "--numeric-owner", "--owner=0", "--group=0",
        "-C", str(ramfs_tree), "-cf", str(filesystem_tar),
        ".",
    ])
    print(f"filesystem.tar gerado: {filesystem_tar} ({filesystem_tar.stat().st_size} bytes)")

    return DZ_IMAGE


def dump_kernel(dzimage: Path, output: Path) -> None:
    """Extrai o ELF do kernel de dentro do dzImage (uso pelo instalador/testes)."""
    build_tools()
    run([str(DZIMAGE_BIN), "extract", str(dzimage), str(output), "/dev/null"])


def main() -> int:
    import argparse
    parser = argparse.ArgumentParser(description="Compila o kernel e o dzImage (via DNU)")
    parser.add_argument("--ramfs", type=Path, default=Path(__file__).resolve().parent.parent / ".build" / "ramfs",
                        help="árvore do initramfs a embutir (padrão: .build/ramfs)")
    args = parser.parse_args()
    try:
        build_dzimage(args.ramfs.resolve())
    except SystemExit as e:
        return int(e.code or 1)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())