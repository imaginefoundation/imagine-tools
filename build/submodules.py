#!/usr/bin/env python3
"""build/submodules.py — verificação e sincronização dos submódulos.

Cada componente do ImagineOS vive em um submódulo na raiz (ver README). As
etapas de build exigem que os submódulos estejam inicializados; este módulo
centraliza essa verificação e explica como resolver quando não estiverem.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

from build.common import fail  # noqa: E402

# Submódulos obrigatórios para o build completo.
REQUIRED = [
    "dnu",                 # kernel e ferramentas (dzImage, DEF, DFS)
    "imagine-tools",       # isobuilder e ferramentas de host
    "imagine-shell",       # /bin/shell
    "imagine-bootloader",  # BOOTX64.EFI + limine.conf
    "imagine-video-server",  # /bin/liminal-server
    "imagine-compositor",  # /bin/liminal-client
    "imagine-desktop",     # /bin/ede-panel
]

# Submódulos opcionais (código legado em migração; o build prossegue sem eles).
OPTIONAL = ["imagine-common"]


def _git(*args: str, cwd: Path = ROOT) -> str:
    result = subprocess.run(
        ["git", *args], cwd=str(cwd), capture_output=True, text=True,
    )
    return result.stdout.strip() if result.returncode == 0 else ""


def submodule_status() -> dict[str, str]:
    """Mapa path -> status (' ', '+', '-' ou 'U') de `git submodule status`."""
    statuses: dict[str, str] = {}
    for line in _git("submodule", "status").splitlines():
        if len(line) >= 2 and line[0] in " -+U":
            statuses[line[1:].split()[0]] = line[0]
    return statuses


def check(require: list[str] | None = None) -> None:
    """Falha com orientação se algum submódulo obrigatório não está pronto."""
    statuses = submodule_status()
    if not statuses:
        # Sem entradas registradas: verifica se os diretórios têm conteúdo.
        missing = [m for m in (require or REQUIRED)
                   if not (ROOT / m).exists()
                   or not any((ROOT / m).iterdir())]
    else:
        missing = [m for m in (require or REQUIRED)
                   if statuses.get(m) in ("-", "U")
                   or (m not in statuses and _is_empty(m))]
    if missing:
        fail(
            "submódulos ausentes ou não inicializados: " + ", ".join(missing) + "\n"
            "  inicialize com: git submodule update --init --recursive"
        )


def _is_empty(path: str) -> bool:
    directory = ROOT / path
    return not directory.exists() or not any(directory.iterdir())


def require_dnu() -> None:
    """Verifica apenas o submódulo DNU (kernel e ferramentas)."""
    check(["dnu"])


def update() -> None:
    """Atualiza todos os submódulos para os commits registrados no índice."""
    print("== atualizando submódulos")
    result = subprocess.run(
        ["git", "submodule", "update", "--init", "--recursive"],
        cwd=str(ROOT),
    )
    if result.returncode != 0:
        fail("git submodule update falhou", result.returncode)


if __name__ == "__main__":
    check()
    print("submódulos ok: " + ", ".join(REQUIRED))
