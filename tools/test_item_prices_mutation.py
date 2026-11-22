#!/usr/bin/env python3
"""Prove the parity test rejects a changed generated value."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CRATE = ROOT / "rust/nookery-item-data"


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="nookery-item-price-mutation-") as directory:
        temp = Path(directory)
        copied_crate = temp / "rust/nookery-item-data"
        copied_crate.parent.mkdir(parents=True)
        shutil.copytree(CRATE, copied_crate)
        shutil.copytree(ROOT / "src/data/item", temp / "src/data/item")

        manifest = copied_crate / "Cargo.toml"
        manifest.write_text(
            """[package]\nname = \"nookery-item-data\"\nversion = \"0.1.0\"\nedition = \"2024\"\nlicense = \"MIT\"\nrust-version = \"1.85\"\n\n[lib]\npath = \"src/lib.rs\"\n"""
        )
        generated = copied_crate / "src/generated.rs"
        original = generated.read_text()
        mutated = original.replace("20,", "21,", 1)
        if mutated == original:
            raise RuntimeError("mutation target was not found")
        generated.write_text(mutated)

        result = subprocess.run(
            [
                "cargo",
                "test",
                "--manifest-path",
                str(manifest),
                "--test",
                "parity",
                "--quiet",
            ],
            cwd=temp,
            text=True,
            capture_output=True,
            check=False,
        )
        if result.returncode == 0:
            print("FAIL: parity tests accepted a mutated generated value")
            return 1
        print("MUTATION DETECTED: parity tests rejected the changed generated value")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
