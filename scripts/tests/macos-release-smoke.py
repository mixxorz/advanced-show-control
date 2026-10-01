#!/usr/bin/env python3
"""Verify real macOS signatures in the portable app and the updater package."""

import argparse
import importlib.util
import os
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("payloads", ROOT / "scripts/installer/validate-release.py")
payloads = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(payloads)


def verify_bundle(bundle):
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(bundle)], check=True)
    for executable in ("advanced-show-control", "UpdateMac"):
        architectures = subprocess.check_output(
            ["lipo", str(bundle / "Contents/MacOS" / executable), "-archs"], text=True
        ).split()
        if not {"arm64", "x86_64"}.issubset(architectures):
            raise ValueError(f"Packaged {executable} is not universal")
    print(f"Valid ad-hoc-signed universal Velopack bundle: {bundle}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--package", type=Path, required=True)
    parser.add_argument("--release-version", required=True)
    parser.add_argument("--channel", required=True)
    parser.add_argument("--work-dir", type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != "darwin" or not args.work_dir.resolve().is_relative_to((ROOT / "dist").resolve()):
        parser.error("macOS signature smoke requires macOS and a work directory under dist")
    payloads.validate_payload(args.archive.read_bytes(), "macos", args.release_version, args.channel, portable=True)
    payloads.validate_payload(args.package.read_bytes(), "macos", args.release_version, args.channel)
    if args.work_dir.exists():
        shutil.rmtree(args.work_dir)
    portable = args.work_dir / "portable"
    portable.mkdir(parents=True)
    subprocess.run(["ditto", "-x", "-k", str(args.archive), str(portable)], check=True)
    verify_bundle(portable / "Advanced Show Control.app")

    # Velopack update archives encode symlinks as __symlink files; restore the app's metadata link.
    bundle = args.work_dir / "update/Advanced Show Control.app"
    with zipfile.ZipFile(args.package) as archive:
        for info in archive.infolist():
            if not info.filename.startswith("lib/app/") or info.is_dir():
                continue
            relative = info.filename.removeprefix("lib/app/")
            destination = bundle / relative
            if not destination.resolve().is_relative_to(bundle.resolve()):
                raise ValueError("Unexpected update archive path")
            destination.parent.mkdir(parents=True, exist_ok=True)
            data = archive.read(info)
            if relative.endswith(".__symlink"):
                if relative != "Contents/MacOS/sq.version.__symlink" or data != b"../Resources/sq.version":
                    raise ValueError("Unexpected application symlink in update package")
                os.symlink(data.decode(), str(destination).removesuffix(".__symlink"))
            else:
                destination.write_bytes(data)
                if mode := (info.external_attr >> 16) & 0o777:
                    destination.chmod(mode)
    for executable in ("advanced-show-control", "UpdateMac"):
        (bundle / "Contents/MacOS" / executable).chmod(0o755)
    verify_bundle(bundle)


if __name__ == "__main__":
    main()
