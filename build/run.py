#!/usr/bin/env python3
"""build/run.py — executa a ISO no QEMU com OVMF (UEFI).

Necessita: qemu-system-x86_64 e o firmware OVMF (OVMF_CODE*.fd).
"""
from __future__ import annotations

import glob
import shutil
from pathlib import Path

from .common import ISO_IMAGE, ROOT, fail, require_tool, run
from . import iso


def find_ovmf() -> Path:
    """Localiza o firmware OVMF nos locais padrão."""
    candidates = [
        ROOT / "OVMF.fd",
        ROOT / "OVMF_CODE.fd",
        "/usr/share/OVMF/OVMF_CODE.fd",
        "/usr/share/OVMF/OVMF_CODE_4M.fd",
        "/usr/share/ovmf/OVMF.fd",
    ]
    for pattern in ["/usr/share/OVMF/OVMF_CODE*.fd", "/usr/share/ovmf/OVMF*.fd"]:
        candidates.extend(sorted(glob.glob(pattern)))
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return Path(candidate)
    fail(
        "firmware OVMF não encontrado (procurei OVMF.fd, /usr/share/OVMF, /usr/share/ovmf).\n"
        "  instale com: apt install ovmf"
    )


def run_qemu(use_disk: bool = False) -> None:
    require_tool("qemu-system-x86_64", "apt install qemu-system-x86")
    image = iso.build_iso()
    firmware = find_ovmf()
    print(f"QEMU com OVMF: {firmware}")
    cmd = [
        "qemu-system-x86_64", "-machine", "q35", "-m", "512M", "-serial", "stdio",
        "-drive", f"if=pflash,format=raw,unit=0,readonly=on,file={firmware}",
        "-cdrom", str(image),
    ]
    run(cmd, check=False)


def main() -> int:
    run_qemu()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
