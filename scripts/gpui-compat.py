#!/usr/bin/env python3
"""Explicit, idempotent patch setup. Never runs inside a Cargo build script."""
import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def source_fingerprint(root):
    """Pin manifests and compiled source, not generated build output or lockfiles."""
    result = hashlib.sha256()
    for path in sorted(root.rglob("*")):
        rel = path.relative_to(root)
        if any(part in {".git", "target"} for part in rel.parts):
            continue
        if not path.is_file() or not (path.name == "Cargo.toml" or path.suffix in {".rs", ".wgsl", ".hlsl", ".metal", ".h"}):
            continue
        result.update(rel.as_posix().encode() + b"\0" + path.read_bytes() + b"\0")
    return result.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--apply", action="store_true")
    parser.add_argument("checkout", type=Path)
    args = parser.parse_args()
    checkout = args.checkout.resolve()
    manifest = json.loads((ROOT / "patches/gpui-compat.json").read_text())
    if not (checkout / "Cargo.toml").is_file():
        parser.error("checkout must be a GPUI-CE repository root")
    patch = ROOT / "patches/gpui-backdrop.patch"
    if digest(patch.read_bytes()) != manifest["patch_sha256"]:
        sys.exit("Backdrop patch changed: review it and regenerate the compatibility manifest.")
    state = source_fingerprint(checkout)
    if state == manifest["patched_source_sha256"]:
        print(f"GPUI_COMPAT_OK: tested source and backdrop hook ({manifest['revision']})")
        return
    if state != manifest["upstream_source_sha256"]:
        sys.exit("GPUI source differs from the tested revision. Nothing was changed. Review/rebase the hook and run renderer checks before updating the compatibility manifest.")
    if args.check:
        sys.exit("GPUI source is the tested upstream revision but the backdrop hook is missing. Run this tool with --apply to this checkout.")
    subprocess.run(["git", "apply", "--check", str(patch)], cwd=checkout, check=True)
    subprocess.run(["git", "apply", str(patch)], cwd=checkout, check=True)
    if source_fingerprint(checkout) != manifest["patched_source_sha256"]:
        sys.exit("Patch result did not match the expected source. Inspect the checkout.")
    print("GPUI_PATCH_APPLIED: tested backdrop hook installed; repeated --apply is a no-op")


if __name__ == "__main__":
    main()
