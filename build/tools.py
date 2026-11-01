#!/usr/bin/env python3
"""build/tools.py — ferramentas do host para o ecossistema Rust Dreamcore.

Substitui os alvos do antigo build/tools.mk. Uso:

    python3 build/tools.py promote      # copia dnu-pack/dnulink/dfs-image do DNU
    python3 build/tools.py dnulink      # .build/dnulink (linker do alvo Dreamcore)
    python3 build/tools.py musl         # constrói a musl portada (third_party)
    python3 build/tools.py rust-target  # instala dnulink p/ cargo + valida spec
    python3 build/tools.py rust-example # compila examples/rust-hello
    python3 build/tools.py rust-std     # aplica a camada dreamcore + exemplo std
    python3 build/tools.py rust-std-new DIR=<dir>  # projeto novo configurado
"""
from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

# Raiz do repositório: três níveis acima de imagine-tools/build/tools.py
ROOT = Path(__file__).resolve().parent.parent.parent

from .common import (  # noqa: E402
    BUILD_DIR,
    DNU_DIR,
    DNU_PACK_BIN,
    DNULINK_BIN,
    DFS_IMAGE_BIN,
    check_tool,
    host_cargo,
)
from .submodules import require_dnu  # noqa: E402

RUST_TARGET = ROOT / "targets" / "x86_64-unknown-dreamcore.json"
RUST_EXAMPLE = ROOT / "examples" / "rust-hello"
RUST_STD_EXAMPLE = ROOT / "examples" / "rust-std-hello"
CARGO_DREAMCORE_DIR = Path.home() / ".cargo" / "dreamcore"

TOOLS = [DNU_PACK_BIN, DNULINK_BIN, DFS_IMAGE_BIN]


def promote() -> None:
    """Constrói as ferramentas do DNU e as copia para .build/."""
    require_dnu()
    check_tool("make", hint="apt install build-essential")
    print("== construindo ferramentas do DNU (dnu-pack, dnulink, dfs-image)")
    subprocess.run(["make", "-C", DNU_DIR, "tools"], check=True)
    BUILD_DIR.mkdir(parents=True, exist_ok=True)
    for src in TOOLS:
        if not src.exists():
            print(f"erro: ferramenta não foi produzida: {src}", file=sys.stderr)
            raise SystemExit(1)
        shutil.copy2(src, BUILD_DIR / src.name)
        print(f"   -> {BUILD_DIR / src.name}")


def dnulink() -> Path:
    """Garante que .build/dnulink existe e o devolve."""
    if DNULINK_BIN.exists():
        return DNULINK_BIN
    promote()
    if not DNULINK_BIN.exists():
        print("erro: dnulink não foi produzido", file=sys.stderr)
        raise SystemExit(1)
    return DNULINK_BIN


def musl() -> None:
    """Constrói a musl portada para o alvo Dreamcore."""
    check_tool("make")
    check_tool("gcc")
    check_tool("patch")
    musl_dir = ROOT / "third_party" / "musl-1.2.6"
    if not musl_dir.exists():
        print(
            "erro: a musl portada não está em third_party/musl-1.2.6.\n"
            "      Obtenha-a do repositório imagine-common e coloque em\n"
            "      third_party/musl-1.2.6 antes de rodar esta etapa.",
            file=sys.stderr,
        )
        raise SystemExit(1)
    print("== construindo musl portada (third_party/musl-1.2.6)")
    subprocess.run(["make", "-C", musl_dir], check=True)
    libc = musl_dir / "lib" / "libc.a"
    if not libc.exists():
        print(f"erro: {libc} não foi produzido", file=sys.stderr)
        raise SystemExit(1)
    print(f"   -> {libc}")


def require_musl() -> None:
    libc = ROOT / "third_party" / "musl-1.2.6" / "lib" / "libc.a"
    if not libc.exists():
        print(
            "erro: a musl portada ainda não foi construída.\n"
            "      Rode: python3 build/tools.py musl",
            file=sys.stderr,
        )
        raise SystemExit(1)


def rust_target() -> None:
    dnulink()
    if not RUST_TARGET.exists():
        print(f"erro: target spec ausente em {RUST_TARGET}", file=sys.stderr)
        raise SystemExit(1)
    check_tool("rustc")
    CARGO_DREAMCORE_DIR.mkdir(parents=True, exist_ok=True)
    shutil.copy2(DNULINK_BIN, CARGO_DREAMCORE_DIR / "dnulink")
    print(f"   -> {CARGO_DREAMCORE_DIR / 'dnulink'}")
    result = subprocess.run(
        ["rustc", "+nightly", "--target", str(RUST_TARGET),
         "--print", "target-spec-json", "-Z", "unstable-options"],
        capture_output=True,
    )
    if result.returncode != 0:
        print("erro: o rustc nightly rejeitou o target spec:", file=sys.stderr)
        print(result.stderr.decode(errors="replace"), file=sys.stderr)
        raise SystemExit(result.returncode)
    print("alvo x86_64-unknown-dreamcore ok")


def rust_example() -> None:
    rust_target()
    if not RUST_EXAMPLE.exists():
        print(f"erro: exemplo ausente em {RUST_EXAMPLE}", file=sys.stderr)
        raise SystemExit(1)
    print(f"== compilando {RUST_EXAMPLE.relative_to(ROOT)}")
    env = os.environ.copy()
    env["RUSTC_BOOTSTRAP"] = "1"
    result = subprocess.run(
        [host_cargo(), "+nightly", "build", "--release", "-Zjson-target-spec"],
        cwd=RUST_EXAMPLE, env=env,
    )
    if result.returncode != 0:
        raise SystemExit(result.returncode)
    print(f"DEF: {RUST_EXAMPLE}/target/x86_64-unknown-dreamcore/release/rust-hello")


def rust_std_patch() -> None:
    check_tool("patch")
    subprocess.run([str(ROOT / "std-dreamcore" / "patches" / "apply.sh")], check=True)


def rust_std() -> None:
    rust_target()
    rust_std_patch()
    if not RUST_STD_EXAMPLE.exists():
        print(f"erro: exemplo ausente em {RUST_STD_EXAMPLE}", file=sys.stderr)
        raise SystemExit(1)
    print(f"== compilando {RUST_STD_EXAMPLE.relative_to(ROOT)} com std")
    env = os.environ.copy()
    env["RUSTC_BOOTSTRAP"] = "1"
    result = subprocess.run(
        [host_cargo(), "+nightly", "build", "--release",
         "-Zjson-target-spec", "-Zbuild-std=std"],
        cwd=RUST_STD_EXAMPLE, env=env,
    )
    if result.returncode != 0:
        raise SystemExit(result.returncode)
    print("DEF (std): "
          f"{RUST_STD_EXAMPLE}/target/x86_64-unknown-dreamcore/release/rust-std-hello")


def rust_std_new(directory: str) -> None:
    if not directory:
        print("uso: python3 build/tools.py rust-std-new DIR=<diretório>", file=sys.stderr)
        raise SystemExit(2)
    rust_target()
    subprocess.run([str(ROOT / "std-dreamcore" / "patches" / "apply.sh")], check=True)
    # Nota: new-project.sh foi movido para imagine-common/std-dreamcore/
    subprocess.run([str(ROOT / "imagine-common" / "std-dreamcore" / "new-project.sh"), directory], check=True)


def main() -> int:
    import argparse

    parser = argparse.ArgumentParser(
        prog="build/tools.py",
        description="Ferramentas do host para o ecossistema Rust Dreamcore.",
    )
    parser.add_argument("command", choices=[
        "promote", "dnulink", "musl", "rust-target", "rust-example",
        "rust-std-patch", "rust-std", "rust-std-new",
    ])
    parser.add_argument("--dir", default="", help="destino do rust-std-new")
    args = parser.parse_args()

    if args.command == "promote":
        promote()
    elif args.command == "dnulink":
        dnulink()
    elif args.command == "musl":
        musl()
    elif args.command == "rust-target":
        rust_target()
    elif args.command == "rust-example":
        rust_example()
    elif args.command == "rust-std-patch":
        rust_std_patch()
    elif args.command == "rust-std":
        rust_std()
    elif args.command == "rust-std-new":
        rust_std_new(args.dir)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except SystemExit:
        raise
    except subprocess.CalledProcessError as exc:
        print(f"erro: comando falhou com código {exc.returncode}", file=sys.stderr)
        raise SystemExit(exc.returncode)
    except KeyboardInterrupt:
        print("\ninterrompido")
        raise SystemExit(130)
