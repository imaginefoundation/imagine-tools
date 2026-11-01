#!/usr/bin/env python3
"""build.py — ponto de entrada do build do ImagineOS.

Substitui o Makefile raiz (e os makefiles em build/): cada etapa é um módulo
Python em `build/`, e o pipeline completo é orquestrado por aqui.

Uso:
    python3 build.py ramfs          # monta a árvore do ramfs (userland + componentes)
    python3 build.py kernel         # compila o dzImage (kernel + initramfs)
    python3 build.py iso            # compila tudo e monta a ISO inicializável
    python3 build.py run            # compila a ISO e inicia no QEMU/OVMF
    python3 build.py clean          # remove artefatos de build

Cada etapa também roda isolada:
    python3 build/kernel.py
    python3 build/userland.py
    python3 build/ramfs.py
    python3 build/iso.py
    python3 build/run.py
"""
from __future__ import annotations

import shutil
import sys
from pathlib import Path

# Raiz do repositório: três níveis acima de imagine-tools/build/build.py
ROOT = Path(__file__).resolve().parent.parent.parent

# Garante que o pacote `build/` (módulos de cada etapa) seja importável.
# Adiciona o diretório pai (imagine-tools/) ao path para que `from build.common`
# encontre imagine-tools/build/common.py.
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from build.common import BUILD_DIR, DISTRO_DIR  # noqa: E402


def main() -> int:
    import argparse
    parser = argparse.ArgumentParser(
        prog="build.py",
        description="Compila o ImagineOS e empacota a ISO inicializável.",
    )
    parser.add_argument(
        "stage",
        choices=["ramfs", "userland", "kernel", "components", "iso", "run", "clean"],
        help="etapa do build",
    )
    args = parser.parse_args()

    if args.stage == "ramfs":
        from build import ramfs
        ramfs.build_tree()
    elif args.stage == "userland":
        from build import userland
        userland.build_all()
    elif args.stage == "kernel":
        from build import kernel
        kernel.build_dzimage(BUILD_DIR / "ramfs")
    elif args.stage == "components":
        from build import components
        components.build_all()
    elif args.stage == "iso":
        from build import iso
        iso.build_iso()
    elif args.stage == "run":
        from build import run
        run.run_qemu()
    elif args.stage == "clean":
        for directory in [BUILD_DIR, DISTRO_DIR,
                          ROOT / "imagine-tools" / "isobuilder" / "target"]:
            if directory.exists():
                print(f"removendo {directory}")
                shutil.rmtree(directory)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except SystemExit:
        raise
    except KeyboardInterrupt:
        print("\ninterrompido")
        raise SystemExit(130)
