#!/usr/bin/env python3
"""build/kernel.py — kernel e dzImage (delegados ao repositório DNU).

O kernel `dreamcore` e o formato DEF são propriedade do submódulo `dnu/`, que
publica um único artefato: `.build/dzImage` (kernel + initramfs no mesmo
contêiner). Este módulo chama o Makefile do DNU com `RAMFS_DIR` apontando para
a árvore montada por `build/ramfs.py`.

O submódulo dnu está no branch `main`, que contém o Makefile e as ferramentas
de host (dzimage, dnu-pack, dnulink, dfs-image) usados aqui e pelo userland.
"""
from __future__ import annotations

import shutil
from pathlib import Path

from .common import (
    BUILD_DIR, DFS_IMAGE_BIN, DNU_BUILD, DNU_DIR, DNU_PACK_BIN, DNULINK_BIN,
    DZ_IMAGE, TARGET, cargo, fail, run, require_nightly,
    require_rust_target, tree_newer,
)


def check_dnu() -> None:
    if not (DNU_DIR / "Makefile").exists():
        fail(
            f"submódulo DNU ausente em {DNU_DIR}.\n"
            "  inicialize com: git submodule update --init dnu"
        )
    if not (DNU_DIR / "src").exists():
        fail(
            "checkout do submódulo DNU está vazio (branch errado?).\n"
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
    """Compila as ferramentas de host do DNU (dzimage, dnu-pack, dnulink, dfs-image)."""
    check_dnu()
    require_nightly()
    # As ferramentas só dependem do Makefile e das fontes do DNU; o cargo check
    # da ABI dentro do `make tools` já valida o bundle. Compilamos direto pelo
    # Makefile do DNU (ele sabe empacotar os bundles da ABI).
    targets = [DNU_PACK_BIN, DNULINK_BIN, DFS_IMAGE_BIN]
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
    """Empacota kernel + initramfs no dzImage (o único artefato de boot)."""
    check_dnu()
    if not ramfs_tree.exists():
        fail(
            "árvore do ramfs ausente; rode a etapa de ramfs antes do dzImage.\n"
            "  execute: python3 build.py ramfs"
        )
    build_kernel()
    build_tools()
    # O Makefile do DNU empacota: RAMFS_DIR -> initramfs dentro do dzImage.
    run(["make", "dzImage", f"RAMFS_DIR={ramfs_tree}"], cwd=DNU_DIR)
    dzimage = DNU_BUILD / "dzImage"
    if not dzimage.exists():
        fail("dzImage não foi gerado pelo DNU")
    BUILD_DIR.mkdir(parents=True, exist_ok=True)
    shutil.copy2(dzimage, DZ_IMAGE)
    run([str(DNU_BUILD / "dzimage"), "info", str(DZ_IMAGE)])
    return DZ_IMAGE


def dump_kernel(dzimage: Path, output: Path) -> None:
    """Extrai o ELF do kernel de dentro do dzImage (uso pelo instalador/testes)."""
    build_tools()
    run([str(DFS_IMAGE_BIN), "--dump-kernel", str(dzimage), str(output)])


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
