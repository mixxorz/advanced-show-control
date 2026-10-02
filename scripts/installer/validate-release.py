#!/usr/bin/env python3
"""Validate Velopack feeds and payloads, then stage only publishable release assets."""

import argparse
import hashlib
import importlib.util
import io
import json
import plistlib
import shutil
import struct
import sys
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("release_metadata", Path(__file__).resolve().parents[1] / "release-metadata.py")
metadata = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(metadata)
PACK_ID = "com.advancedshowcontrol.app"


def validate_feed(feed, files, version):
    assets = feed.get("Assets", [])
    if not assets or not any(asset.get("Type") == "Full" for asset in assets):
        raise ValueError("Release feed must include a full update package")
    names = []
    for asset in assets:
        name = asset.get("FileName", "")
        if Path(name).name != name or name not in files or not name.endswith(".nupkg") or name in names:
            raise ValueError(f"Invalid or missing feed package: {name}")
        if asset.get("PackageId") != PACK_ID or asset.get("Version") != version or asset.get("Type") not in ("Full", "Delta"):
            raise ValueError(f"Wrong feed identity, version or type: {name}")
        data = files[name]
        if asset.get("Size") != len(data):
            raise ValueError(f"Feed package size mismatch: {name}")
        for algorithm, field in [("sha1", "SHA1"), ("sha256", "SHA256")]:
            if asset.get(field, "").upper() != hashlib.new(algorithm, data).hexdigest().upper():
                raise ValueError(f"Feed package {field} mismatch: {name}")
        names.append(name)
    return names


def require_x64_pe(data):
    if len(data) < 64 or data[:2] != b"MZ":
        raise ValueError("Windows payload must contain a native PE executable")
    offset = struct.unpack_from("<I", data, 60)[0]
    if len(data) < offset + 6 or data[offset:offset + 4] != b"PE\x00\x00" or struct.unpack_from("<H", data, offset + 4)[0] != 0x8664:
        raise ValueError("Windows application must target x64")


def require_universal_macho(data):
    if len(data) < 8:
        raise ValueError("macOS payload must contain universal Mach-O binaries")
    formats = {b"\xca\xfe\xba\xbe": (">", 20), b"\xbe\xba\xfe\xca": ("<", 20),
               b"\xca\xfe\xba\xbf": (">", 32), b"\xbf\xba\xfe\xca": ("<", 32)}
    if data[:4] not in formats:
        raise ValueError("macOS application and UpdateMac must both be universal")
    endian, stride = formats[data[:4]]
    count = struct.unpack_from(endian + "I", data, 4)[0]
    if len(data) < 8 + count * stride:
        raise ValueError("Truncated universal Mach-O header")
    cpus = {struct.unpack_from(endian + "I", data, 8 + index * stride)[0] for index in range(count)}
    if not {0x01000007, 0x0100000C}.issubset(cpus):
        raise ValueError("macOS application and UpdateMac must include x86_64 and arm64")


def validate_payload(data, platform, version, channel, portable=False):
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        names = set(archive.namelist())
        if platform == "windows":
            prefix = "current/" if portable else "lib/app/"
            required = {prefix + "advanced-show-control.exe", prefix + "sq.version"}
            required |= {"Update.exe", "Advanced Show Control.exe", ".portable"} if portable else {prefix + "Squirrel.exe"}
            version_path = prefix + "sq.version"
            main_exe = "advanced-show-control.exe"
        elif platform == "macos":
            prefix = "Advanced Show Control.app/" if portable else "lib/app/"
            required = {prefix + "Contents/MacOS/advanced-show-control", prefix + "Contents/MacOS/UpdateMac",
                        prefix + "Contents/Resources/sq.version", prefix + "Contents/Info.plist", prefix + "Contents/_CodeSignature/CodeResources"}
            version_path = prefix + "Contents/Resources/sq.version"
            main_exe = "Contents/MacOS/advanced-show-control"
        else:
            raise ValueError(f"Unknown platform: {platform}")
        if missing := required - names:
            raise ValueError(f"Missing {platform} Velopack payload files: {sorted(missing)}")
        root = ET.fromstring(archive.read(version_path))
        values = {node.tag.rsplit("}", 1)[-1]: node.text for node in root.iter()}
        if any(values.get(key) != value for key, value in {
            "id": PACK_ID, "version": version, "channel": channel, "mainExe": main_exe
        }.items()):
            raise ValueError("Packaged sq.version has the wrong identity, version, channel or entry point")
        if platform == "windows":
            if values.get("osMinVersion") != "10.0.15063" or values.get("machineArchitecture") != "x64":
                raise ValueError("Windows update metadata must require x64 Windows 10 build 15063 or newer")
            require_x64_pe(archive.read(prefix + "advanced-show-control.exe"))
        else:
            for executable in ("advanced-show-control", "UpdateMac"):
                require_universal_macho(archive.read(prefix + "Contents/MacOS/" + executable))
            plist = plistlib.loads(archive.read(prefix + "Contents/Info.plist"))
            if any(plist.get(key) != value for key, value in {
                "CFBundleIdentifier": PACK_ID, "CFBundleVersion": version,
                "CFBundleShortVersionString": version, "LSMinimumSystemVersion": "15.0"
            }.items()):
                raise ValueError("macOS bundle must preserve identity, release version and macOS 15 minimum")


def validate_and_stage(directory, platform, release_id, version, stage=None):
    version = metadata.validate_version(version)
    result = metadata.resolve_metadata(release_id, version)
    channel = result["windows_channel" if platform == "windows" else "macos_channel"]
    files = {path.name: path.read_bytes() for path in directory.iterdir() if path.is_file()}
    feed_name = f"releases.{channel}.json"
    if feed_name not in files:
        raise ValueError(f"Missing channel feed: {feed_name}")
    feed = json.loads(files[feed_name])
    package_names = validate_feed(feed, files, version)
    if set(package_names) != {name for name in files if name.endswith(".nupkg")}:
        raise ValueError("Output contains stale or unreferenced update packages")
    for asset in feed["Assets"]:
        if asset["Type"] == "Full":
            validate_payload(files[asset["FileName"]], platform, version, channel)
    portable = f"{PACK_ID}-{channel}-Portable.zip"
    if portable not in files:
        raise ValueError("Missing Velopack portable archive")
    validate_payload(files[portable], platform, version, channel, portable=True)
    suffix = "Windows_x64" if platform == "windows" else "macOS_universal"
    publish = {name: name for name in [feed_name, *package_names]}
    publish[portable] = f"Advanced-Show-Control_{release_id}_{suffix}.zip"
    if platform == "windows":
        msi = f"{PACK_ID}-{channel}.msi"
        setup = f"{PACK_ID}-{channel}-Setup.exe"
        if msi not in files or setup not in files:
            raise ValueError("Windows release requires both MSI and Setup.exe")
        publish[msi] = f"Advanced-Show-Control_{release_id}_{suffix}.msi"
        publish[setup] = f"Advanced-Show-Control_{release_id}_{suffix}_Setup.exe"
    if stage:
        if stage.resolve() == directory.resolve():
            raise ValueError("Staged assets must be separate from Velopack's internal output")
        if stage.exists():
            shutil.rmtree(stage)
        stage.mkdir(parents=True)
        for source, destination in publish.items():
            shutil.copyfile(directory / source, stage / destination)
    return list(publish.values())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--platform", choices=("windows", "macos"), required=True)
    parser.add_argument("--release-id", required=True)
    parser.add_argument("--release-version", required=True)
    parser.add_argument("--stage", type=Path)
    args = parser.parse_args()
    try:
        for name in validate_and_stage(args.directory, args.platform, args.release_id, args.release_version, args.stage):
            print(name)
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, ET.ParseError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
