#!/usr/bin/env python3
"""build/userland.py — programas de usuário (Rust, C e libc).

Compila os programas de userspace para o alvo bare-metal x86_64-unknown-none
e empacota no formato **DEF** (magic `DNUF`), o único que o kernel carrega:
  * apps principais: init, getty, shell, liminal-server, liminal-client
  * utilitários Rust: ls, cat, ... em imagine-tools/utilities/
  * utilitários C: hello_c
  * libc estática (libdreamcore.a), objetos crt.

Cada binário é linkado como ELF estático (via `cargo` com um manifest gerado
por programa, replicando os flags da antiga regra do rustc) e o `dnu-pack`
converte o ELF no arquivo final DEF (sem extensão, como são instalados em
/bin e /sbin).
"""
from __future__ import annotations

import shutil
import subprocess
import textwrap
from pathlib import Path

from .common import (
    BUILD_DIR, COMMON_LEGACY_DIR, DNU_PACK_BIN, LINKER_LD, PROGRAMS_DIR, ROOT, TARGET,
    TOOLS_DIR, USER_API_DIR, USER_API_TARGET, USER_C_UTILITIES, USER_DIR,
    USER_LIBC_DIR, USER_RUNTIME_DIR, USER_RUNTIME_TARGET, USER_UTILITIES,
    PROGRAMS_SHELL_DIR, PROGRAMS_LIMINAL_SERVER_DIR, PROGRAMS_LIMINAL_CLIENT_DIR,
    cargo, fail, require_nightly, require_rust_target, require_tool,
    tree_newer,
)

# Manifests gerados por programa (o cargo é a única forma confiável de ligar
# as rlibs da API/runtime no nightly — o rustc direto falha com o stub de
# metadados do alvo bare-metal).
GEN_DIR = BUILD_DIR / "user" / "gen"
GEN_TARGET = BUILD_DIR / "user" / "gen-target"


def _find_rlib(target_dir: Path, pattern: str) -> Path:
    """Localiza a rlib mais recente no target-dir."""
    matches = sorted(target_dir.rglob(pattern), key=lambda p: p.stat().st_mtime)
    return matches[-1] if matches else target_dir / "__missing__.rlib"


def build_api_rlib() -> Path:
    """Compila a API do userspace (crate imagineos-userland) para o alvo."""
    require_nightly()
    require_rust_target()
    rlib = _find_rlib(USER_API_TARGET, "libimagineos-*.rlib")
    if rlib.exists():
        return rlib
    cargo(True, "build", "--manifest-path", str(USER_API_DIR / "Cargo.toml"),
          "--target", TARGET, "--release", "--target-dir", str(USER_API_TARGET))
    rlib = _find_rlib(USER_API_TARGET, "libimagineos-*.rlib")
    if not rlib.exists():
        fail("rlib da API não foi gerada")
    return rlib


def build_runtime_rlib() -> Path:
    """Compila o runtime Rust de userspace (imagineos_rt)."""
    build_api_rlib()
    rlib = _find_rlib(USER_RUNTIME_TARGET, "libimagineos_rt-*.rlib")
    if rlib.exists():
        return rlib
    cargo(True, "build", "--manifest-path", str(USER_RUNTIME_DIR / "Cargo.toml"),
          "--target", TARGET, "--release", "--target-dir", str(USER_RUNTIME_TARGET))
    rlib = _find_rlib(USER_RUNTIME_TARGET, "libimagineos_rt-*.rlib")
    if not rlib.exists():
        fail("rlib do runtime não foi gerada")
    return rlib


def pack_def(elf: Path, output: Path) -> None:
    """Converte o ELF linkado no arquivo DEF final (o formato do kernel)."""
    run_pack(elf, output)


def run_pack(elf: Path, output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    run_dnu_pack(elf, output)


def run_dnu_pack(elf: Path, output: Path) -> None:
    if not DNU_PACK_BIN.exists():
        fail(
            f"dnu-pack ausente em {DNU_PACK_BIN}.\n"
            "  rode a etapa do kernel antes do userland: python3 build.py kernel"
        )
    from .common import run
    run([str(DNU_PACK_BIN), str(elf), str(output)])


def _gen_crate(name: str, source: Path, out_elf: Path) -> None:
    """Gera um manifest temporário e compila o programa com cargo.

    Replicando a antiga regra do rustc: --edition 2024, panic=abort,
    relocation-model=static e linker script do userspace. Multi-módulo
    (shell, liminal) usa o diretório de fontes como crate-raiz via
    `path` no manifest.
    """
    crate_dir = GEN_DIR / name
    crate_dir.mkdir(parents=True, exist_ok=True)
    (crate_dir / ".cargo").mkdir(exist_ok=True)
    # Config local: os flags de link do userspace (idênticos à regra antiga).
    (crate_dir / ".cargo" / "config.toml").write_text(
        "[target.x86_64-unknown-none]\n"
        'rustflags = ["-C", "relocation-model=static",\n'
        f'    "-C", "link-arg=-T{LINKER_LD}"]\n'
    )
    is_dir = source.is_dir()
    bin_path = source / "main.rs" if is_dir else source
    manifest = f"""[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

[workspace]

[[bin]]
name = "{name}"
path = "{bin_path}"

[profile.release]
panic = "abort"
lto = true
codegen-units = 1

[dependencies]
imagineos = {{ path = "{USER_API_DIR}" }}
imagineos_rt = {{ path = "{USER_RUNTIME_DIR}", package = "imagineos-rt" }}
"""
    (crate_dir / "Cargo.toml").write_text(manifest)
    if is_dir:
        # Copia os módulos irmãos (main.rs declara `mod ...` de arquivos
        # locais) para o diretório do crate gerado.
        for module in source.glob("*.rs"):
            if module.name != "main.rs" and module.name != "tests.rs":
                shutil.copy2(module, crate_dir / module.name)
    cargo(True, "build", "--release", "--target", TARGET,
          "--target-dir", str(GEN_TARGET), cwd=crate_dir)
    elf = GEN_TARGET / TARGET / "release" / name
    if not elf.exists():
        fail(f"ELF não foi gerado pelo cargo: {elf}")
    out_elf.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(elf, out_elf)


def compile_rust_program(source: Path, crate_name: str, output_elf: Path) -> None:
    """Linka um programa Rust de userspace como ELF estático (DEF pronto)."""
    build_api_rlib()
    build_runtime_rlib()
    _gen_crate(crate_name, source, output_elf)


def build_rust_utilities() -> list[Path]:
    """Compila os utilitários Rust de arquivo único em imagine-tools/utilities/."""
    outputs = []
    for utility in USER_UTILITIES:
        source = UTILITIES_SRC(utility)
        if not source.exists():
            continue
        elf = USER_DIR / "utilities" / f"{utility}.elf"
        target = USER_DIR / "utilities" / utility
        if tree_newer(source, elf) or not target.exists():
            compile_rust_program(source, utility, elf)
            pack_def(elf, target)
        outputs.append(target)
    return outputs


def UTILITIES_SRC(name: str) -> Path:
    # Primeiro procura em imagine-common/utilities/, depois em imagine-tools/utilities/
    src = COMMON_LEGACY_DIR / "utilities" / f"{name}.rs"
    if src.exists():
        return src
    return TOOLS_DIR / "utilities" / f"{name}.rs"


def compile_libc() -> list[Path]:
    """Compila a libc estática (libdreamcore.a) com cc."""
    require_tool("cc", "instale um compilador C (ex.: apt install gcc)")
    require_tool("ar", "instale binutils (apt install binutils)")
    include_dir = USER_LIBC_DIR / "include"
    # Os headers do freestanding do GCC (stdint.h etc.) entram via -isystem.
    gcc_include = subprocess.run(["cc", "-print-file-name=include"],
                                 capture_output=True, text=True).stdout.strip()
    objects = []
    static_lib = USER_DIR / "libdreamcore.a"
    for c_file in sorted(USER_LIBC_DIR.glob("*.c")):
        obj = USER_DIR / f"{c_file.stem}.o"
        if tree_newer(c_file, obj):
            obj.parent.mkdir(parents=True, exist_ok=True)
            from .common import run
            run(["cc", "-ffreestanding", "-fno-stack-protector", "-fno-pic",
                 "-fno-pie", "-mno-red-zone", "-nostdinc",
                 f"-I{include_dir}", f"-isystem{gcc_include}",
                 "-c", str(c_file), "-o", str(obj)])
        objects.append(obj)
    from .common import run
    static_lib.parent.mkdir(parents=True, exist_ok=True)
    run(["ar", "rcs", str(static_lib), *[str(o) for o in objects]])
    return objects


def build_main_programs() -> dict[str, Path]:
    """Compila init, getty, shell e a stack Liminal em DEF."""
    results = {}
    single_file_apps = [
        (COMMON_LEGACY_DIR / "init" / "init.rs", "sbin", "init"),
    ]
    for source, subdir, name in single_file_apps:
        if not source.exists():
            print(f"aviso: fonte do programa ausente, pulando: {source}")
            continue
        elf = USER_DIR / subdir / f"{name}.elf"
        target = USER_DIR / subdir / name
        needs_build = tree_newer(source, elf) or not target.exists()
        print(f"DEBUG: {name}: tree_newer={needs_build}, elf_exists={elf.exists()}, target_exists={target.exists()}")
        if needs_build or not target.exists():
            print(f"DEBUG: compilando e empacotando {name}")
            compile_rust_program(source, name, elf)
            pack_def(elf, target)
            print(f"DEBUG: {name} empacotado")
        else:
            print(f"DEBUG: {name} já está atualizado, pulando")
        results[name] = target
    multi_module_apps = []
    if PROGRAMS_SHELL_DIR and PROGRAMS_SHELL_DIR.exists():
        multi_module_apps.append((PROGRAMS_SHELL_DIR, "bin", "shell"))
    # Servidor de janelas/compositor em userspace.
    GUI_DIR = COMMON_LEGACY_DIR / "gui"
    if GUI_DIR.exists():
        multi_module_apps.append((GUI_DIR, "bin", "ws_server"))
    # Shell gráfico.
    EXPLORER_DIR = COMMON_LEGACY_DIR / "explorer"
    if EXPLORER_DIR.exists():
        multi_module_apps.append((EXPLORER_DIR, "bin", "explorer"))
    if PROGRAMS_LIMINAL_SERVER_DIR and PROGRAMS_LIMINAL_SERVER_DIR.exists():
        multi_module_apps.append((PROGRAMS_LIMINAL_SERVER_DIR, "bin", "liminal-server"))
    if PROGRAMS_LIMINAL_CLIENT_DIR and PROGRAMS_LIMINAL_CLIENT_DIR.exists():
        multi_module_apps.append((PROGRAMS_LIMINAL_CLIENT_DIR, "bin", "liminal-client"))
    for source_dir, subdir, name in multi_module_apps:
        elf = USER_DIR / subdir / f"{name}.elf"
        target = USER_DIR / subdir / name
        if tree_newer(source_dir, elf) or not target.exists():
            compile_rust_program(source_dir, name.replace("-", "_"), elf)
            pack_def(elf, target)
        results[name] = target
    return results


def build_c_utility(name: str) -> Path:
    """Compila um utilitário C contra a libc do ImagineOS e empacota em DEF."""
    require_tool("cc", "instale um compilador C (ex.: apt install gcc)")
    require_tool("ar", "instale binutils (apt install binutils)")
    compile_libc()
    source = COMMON_LEGACY_DIR / "utilities" / f"{name}.c"
    if not source.exists():
        source = TOOLS_DIR / "utilities" / f"{name}.c"
    if not source.exists():
        fail(f"fonte C do utilitário ausente: {name}.c")
    include_dir = USER_LIBC_DIR / "include"
    obj_dir = USER_DIR / "c"
    obj_dir.mkdir(parents=True, exist_ok=True)
    obj = obj_dir / f"{name}.o"
    elf = USER_DIR / "utilities" / f"{name}.elf"
    target = USER_DIR / "utilities" / name
    if tree_newer(source, elf) or not target.exists():
        from .common import run
        objects = compile_libc()
        run(["cc", "-ffreestanding", "-fno-stack-protector", "-fno-pic",
             "-fno-pie", "-mno-red-zone", "-nostdinc", f"-I{include_dir}",
             "-c", str(source), "-o", str(obj)])
        # O programa C linka estático com a libc do ImagineOS (todos os
        # objetos compilados) — o crt.o fornece _start e a runtime C.
        run(["cc", "-ffreestanding", "-fno-stack-protector", "-fno-pic",
             "-fno-pie", "-mno-red-zone", "-nostdinc", f"-I{include_dir}",
             "-nostdlib", "-static", "-no-pie",
             f"-Wl,-T{LINKER_LD}", "-Wl,--build-id=none",
             str(obj), str(USER_DIR / "libdreamcore.a"), "-o", str(elf)])
        pack_def(elf, target)
    return target


def build_all() -> dict[str, object]:
    """Compila tudo que entra na árvore do ramfs."""
    print("== userland: programas principais (init, shell, explorer, ws_server)")
    programs = build_main_programs()
    print("== userland: libc estática")
    compile_libc()
    return {"programs": programs, "utilities": {}, "c_utilities": []}


def main() -> int:
    try:
        build_all()
    except SystemExit as e:
        return int(e.code or 1)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
