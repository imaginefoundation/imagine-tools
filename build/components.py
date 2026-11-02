#!/usr/bin/env python3
"""build/components.py — compila os componentes dos submódulos.

Cada componente é um submódulo na raiz (imagine-shell, imagine-video-server,
imagine-compositor, imagine-desktop) com um programa compilado para o alvo
bare-metal x86_64-unknown-none. O ELF resultante é empacotado em DEF pelo
`dnu-pack` e instalado no ramfs em /bin.

Componentes em migração (stub compilável mantendo o contrato `_start`):
  * imagine-shell        -> /bin/shell
  * imagine-video-server -> /bin/liminal-server
  * imagine-compositor   -> /bin/liminal-client
  * imagine-desktop      -> /bin/ede-panel

Programas locais em imagine-base/shell/ (movidos de common/programs/):
  * shell        -> /bin/shell
"""
from __future__ import annotations

from pathlib import Path

from .common import (
    COMPONENT_BINARIES, DNU_PACK_BIN, PROGRAMS_SHELL_DIR, ROOT, TARGET,
    USER_DIR, cargo, fail, require_nightly, require_rust_target, run, tree_newer,
)
from . import userland

# Programas locais em imagine-base/shell/ com seus nomes instalados
LOCAL_PROGRAMS = {}
if PROGRAMS_SHELL_DIR and PROGRAMS_SHELL_DIR.exists():
    LOCAL_PROGRAMS["shell"] = (PROGRAMS_SHELL_DIR, "shell")
# liminalServer e liminalClient foram removidos (não existem mais)


def build_component(name: str) -> Path:
    """Compila um submódulo de componente e empacota o ELF em DEF.

    Devolve o caminho do arquivo DEF (sem extensão), pronto para /bin.
    Retorna None se o submódulo não existe (não foi inicializado).
    """
    component_dir = ROOT / name
    manifest = component_dir / "Cargo.toml"
    if not manifest.exists():
        print(f"aviso: submódulo {name} não encontrado, pulando...")
        return None
    require_nightly()
    require_rust_target()
    relative, install_name, _destination = COMPONENT_BINARIES[name]
    elf = component_dir / relative
    if tree_newer(component_dir / "src", elf) or tree_newer(manifest, elf) or not elf.exists():
        cargo(True, "build", "--release", "--target", TARGET, cwd=component_dir)
    if not elf.exists():
        fail(f"ELF do componente {name} não foi gerado: {elf}")
    target = component_dir / (relative.rsplit("/", 1)[-1] + ".def")
    pack = Path(DNU_PACK_BIN)
    if not pack.exists():
        fail(
            "ferramenta dnu-pack ausente; rode a etapa do kernel antes dos "
            "componentes (python3 build.py kernel)."
        )
    run([str(pack), str(elf), str(target)])
    # O nome instalado no ramfs é o que /bin recebe (shell, liminal-server…).
    installed = target.with_name(install_name)
    if installed != target:
        import shutil
        shutil.copy2(target, installed)
    return installed


def build_local_program(name: str) -> Path:
    """Compila um programa local em imagine-base/shell/ e empacota em DEF.

    Devolve o caminho do arquivo DEF (sem extensão), pronto para /bin.
    Usa a mesma abordagem de _gen_crate do userland
    para criar um manifest temporário e compilar.
    """
    source_dir, install_name = LOCAL_PROGRAMS[name]
    require_nightly()
    require_rust_target()

    # O programa não tem Cargo.toml próprio; usamos o _gen_crate do userland
    # para criar um manifest temporário e compilar.
    elf = USER_DIR / f"{install_name}.elf"
    out_elf = ROOT / ".build" / "user" / "gen-target" / TARGET / "release" / install_name
    out_def = ROOT / ".build" / "user" / "bin" / install_name

    if tree_newer(source_dir, out_elf) or not out_def.exists():
        userland.compile_rust_program(source_dir, install_name.replace("-", "_"), out_elf)
        userland.pack_def(out_elf, out_def)

    return out_def


def build_all() -> dict[str, Path]:
    """Compila todos os componentes; devolve {nome instalado: caminho DEF}."""
    results = {}
    for name in COMPONENT_BINARIES:
        path = build_component(name)
        if path is not None:
            results[path.name] = path
    for name in LOCAL_PROGRAMS:
        path = build_local_program(name)
        results[path.name] = path
    return results


def main() -> int:
    try:
        for name, path in build_all().items():
            print(f"componente pronto: {name} -> {path}")
    except SystemExit as e:
        return int(e.code or 1)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())