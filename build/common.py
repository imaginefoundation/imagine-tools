#!/usr/bin/env python3
"""Variáveis e helpers compartilhados por todos os módulos de build.

Este é o equivalente Python de `build/common.mk` (o Makefile que ele substitui
define aqui os nomes de diretórios, ferramentas e artefatos de saída).
"""
from __future__ import annotations

import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

# Raiz do repositório: um nível acima de build/ (que está em imagine-tools/build/)
ROOT = Path(__file__).resolve().parent.parent.parent

# --- Identidade e alvo --------------------------------------------------------
K_NAME = "dreamcore"
CODENAME = "astrid"
TARGET = "x86_64-unknown-none"

# --- Ferramentas --------------------------------------------------------------
# O kernel DNU exige nightly (o crate `limine` usa `#![feature]`).
NIGHTLY = "nightly"

# --- Diretórios e artefatos de saída ------------------------------------------
BUILD_DIR = ROOT / ".build"
DISTRO_DIR = BUILD_DIR / "distro"

# Kernel (repositório DNU, submódulo em dnu/)
DNU_DIR = ROOT / "dnu"
DNU_BUILD = DNU_DIR / ".build"
DZ_IMAGE = BUILD_DIR / "dzImage"
DZIMAGE_BIN = DNU_BUILD / "tools" / "dzimage"
DNUDEBUG_BIN = DNU_BUILD / "tools" / "dnudebug"
DNUTEST_BIN = DNU_BUILD / "tools" / "dnutest"
DNUBOOT_BIN = DNU_BUILD / "tools" / "dnuboot"
DNU_PACK_BIN = DNU_BUILD / "tools" / "dnu_pack"

# Árvore montada que o DNU empacota no initramfs (RAMFS_DIR para o DNU).
RAMFS_TREE = BUILD_DIR / "ramfs"

# Componentes (submódulos com programas compilados para o ImagineOS)
COMPONENTS_DIR = ROOT
COMPONENT_BINARIES = {
    # componente: (caminho do ELF dentro do submódulo, nome instalado, destino)
    "imagine-shell": ("target/x86_64-unknown-none/release/imagine-shell", "shell", "bin"),
    "imagine-video-server": ("target/x86_64-unknown-none/release/imagine-video-server", "liminal-server", "bin"),
    "imagine-compositor": ("target/x86_64-unknown-none/release/imagine-compositor", "liminal-client", "bin"),
    "imagine-desktop": ("target/x86_64-unknown-none/release/imagine-desktop", "ede-panel", "bin"),
}

# ISO, imagem de disco e disco do instalador.
ISO_DIR = BUILD_DIR / "iso"
ISO_IMAGE = DISTRO_DIR / f"{K_NAME}-{time.strftime('%Y-%m-%d-%H-%M')}-{CODENAME}.iso"
DISK_IMAGE = BUILD_DIR / "imagineos-disk.img"
INSTALL_TARGET = BUILD_DIR / "installer-target.img"

# Bootloader (submódulo imagine-bootloader ou imagine-base/config/boot/)
LIMINE_DIR = ROOT / "imagine-base" / "config" / "boot"
BOOT_DIR = ROOT / "boot"

# --- Diretórios de origem reorganizados por submódulo -------------------------
# imagine-base: infraestrutura principal (ABI, API, libc, runtimes, config, gui)
COMMON_DIR = ROOT / "imagine-base"
RAMFS_ROOT = COMMON_DIR / "ramfs"
RAMFS_DIRS = ["bin", "dev", "etc", "sbin", "home", "system/fonts", "system/install", "tmp", "usr"]

# Programas compartilhados (init, getty) vivem em imagine-common/programs/
COMMON_LEGACY_DIR = ROOT / "imagine-common"
PROGRAMS_DIR = COMMON_LEGACY_DIR / "programs"

# Shell (console) vive em imagine-common/console/
SHELL_DIR = COMMON_LEGACY_DIR / "console"

# Libc e API vivem em imagine-base/userland/
USERLAND_DIR = COMMON_DIR / "userland"
USER_API_DIR = USERLAND_DIR / "api"
USER_API_TARGET = BUILD_DIR / "user" / "api-target"
USER_RUNTIME_DIR = USERLAND_DIR / "runtimes" / "ImagineRustRuntime"
USER_RUNTIME_TARGET = BUILD_DIR / "user" / "runtime-target"
USER_CRT_DIR = USERLAND_DIR / "runtimes" / "ImagineCRuntime"
USER_CRT_TARGET = BUILD_DIR / "user" / "crt-target"
USER_LIBC_DIR = COMMON_DIR / "libc"
LINKER_LD = USERLAND_DIR / "linker.ld"
ABI_DIR = COMMON_DIR / "abi"

# imagine-tools: ferramentas de host e utilitários C/Rust
TOOLS_DIR = ROOT / "imagine-tools"
UTILITIES_DIR = COMMON_LEGACY_DIR / "utilities"
PROGRAMS_SHELL_DIR = SHELL_DIR
# liminal_server e liminal_client foram removidos (não existem mais)
PROGRAMS_LIMINAL_SERVER_DIR = None
PROGRAMS_LIMINAL_CLIENT_DIR = None
BUILD_SCRIPTS_DIR = TOOLS_DIR / "build"

USER_UTILITIES = [
    "cat", "dmesg", "distroinstall", "fdtest", "forktest", "globalconf",
    "grep", "kill", "ls", "mkdir", "ps", "rm", "shutdown", "su", "sudo",
    "systemfetch", "touch", "uname", "vi", "hello", "top",
]
USER_C_UTILITIES = ["hello_c"]

USER_DIR = BUILD_DIR / "user"


class BuildError(Exception):
    """Erro de build com mensagem amigável e código de saída."""


def fail(message: str, code: int = 1):
    print(f"erro: {message}", file=sys.stderr)
    raise SystemExit(code)


def run(cmd, cwd=None, env=None, check=True, capture=False):
    """Roda um comando, propagando a saída; falhas viram BuildError."""
    printable = " ".join(str(c) for c in cmd)
    print(f"  $ {printable}")
    result = subprocess.run(
        [str(c) for c in cmd],
        cwd=str(cwd) if cwd else None,
        env={**os.environ, **{k: str(v) for k, v in (env or {}).items()}} if env else None,
        check=False,
        capture_output=capture,
        text=capture,
    )
    if check and result.returncode != 0:
        if capture:
            sys.stdout.write(result.stdout or "")
            sys.stderr.write(result.stderr or "")
        raise BuildError(f"comando falhou ({result.returncode}): {printable}")
    return result


def require_tool(name: str, hint: str = "", code: int = 2):
    """Exige uma ferramenta no PATH; senão explica e sai com código de erro."""
    path = shutil.which(name)
    if path is None:
        message = f"ferramenta ausente: {name}"
        if hint:
            message += f"\n  instale com: {hint}"
        fail(message, code)
    return path


# Alias usado por build/tools.py (mesma semântica, nome curto).
check_tool = require_tool


def host_cargo() -> str:
    """Cargo para ferramentas de host (stable)."""
    result = subprocess.run(["rustup", "which", "cargo"], capture_output=True, text=True)
    if result.returncode != 0 or not result.stdout.strip():
        fail("cargo não encontrado no PATH (instale o Rust: rustup)")
    return result.stdout.strip()


def require_nightly() -> None:
    """Verifica que o toolchain nightly está instalado (exigido pelo kernel)."""
    result = subprocess.run(["rustup", "toolchain", "list"], capture_output=True, text=True)
    if "nightly" not in result.stdout:
        fail(
            "toolchain Rust nightly ausente (o kernel DNU compila só em nightly).\n"
            "  instale com: rustup toolchain install nightly"
        )


def require_rust_target() -> None:
    """Verifica que o alvo bare-metal x86_64-unknown-none está instalado."""
    result = subprocess.run(["rustup", "target", "list", "--installed"], capture_output=True, text=True)
    if TARGET not in result.stdout:
        fail(f"alvo Rust {TARGET} ausente.\n  instale com: rustup target add {TARGET}")


def cargo(nightly: bool, *args, cwd=None, env=None):
    cmd = ["rustup", "run", NIGHTLY if nightly else "stable", "cargo", *args]
    return run(cmd, cwd=cwd, env=env)


def rustc(nightly: bool, *args):
    return run(["rustup", "run", NIGHTLY if nightly else "stable", "rustc", *args])


def newer(source: Path, target: Path) -> bool:
    """True se `source` é mais novo que `target` (ou `target` não existe)."""
    if not target.exists():
        return True
    if not source.exists():
        return False
    return source.stat().st_mtime > target.stat().st_mtime


def tree_newer(source: Path, target: Path) -> bool:
    """True se qualquer arquivo de `source` (recursivo) é mais novo que `target`."""
    if not target.exists():
        return True
    latest = 0.0
    for path in source.rglob("*"):
        if path.is_file():
            latest = max(latest, path.stat().st_mtime)
    return latest > target.stat().st_mtime
