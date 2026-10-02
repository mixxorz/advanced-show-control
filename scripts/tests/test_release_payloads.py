import hashlib
import importlib.util
import io
import json
import plistlib
import struct
import unittest
import zipfile
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "release_payloads", Path(__file__).resolve().parents[1] / "installer/validate-release.py"
)
payloads = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(payloads)
VERSION = "12.177.49536"


def zip_bytes(files):
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w") as archive:
        for name, content in files.items():
            archive.writestr(name, content)
    return output.getvalue()


def metadata(channel, main_exe):
    minimum = "<osMinVersion>10.0.15063</osMinVersion><machineArchitecture>x64</machineArchitecture>" if channel.startswith("win-") else ""
    return f"<package><metadata><id>com.advancedshowcontrol.app</id><version>{VERSION}</version><channel>{channel}</channel><mainExe>{main_exe}</mainExe>{minimum}</metadata></package>"


def pe_x64():
    data = bytearray(160)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 60, 128)
    data[128:132] = b"PE\x00\x00"
    struct.pack_into("<H", data, 132, 0x8664)
    return bytes(data)


def universal_macho():
    return struct.pack(">II", 0xCAFEBABE, 2) + b"".join(
        struct.pack(">IIIII", cpu, 0, 0, 0, 0) for cpu in (0x01000007, 0x0100000C)
    )


def windows_files(portable=False):
    prefix = "current/" if portable else "lib/app/"
    files = {prefix + "advanced-show-control.exe": pe_x64(), prefix + "sq.version": metadata("win-x64-stable", "advanced-show-control.exe")}
    if portable:
        files.update({"Update.exe": pe_x64(), "Advanced Show Control.exe": pe_x64(), ".portable": ""})
    else:
        files[prefix + "Squirrel.exe"] = pe_x64()
    return files


def macos_files(portable=False):
    prefix = "Advanced Show Control.app/" if portable else "lib/app/"
    return {
        prefix + "Contents/MacOS/advanced-show-control": universal_macho(),
        prefix + "Contents/MacOS/UpdateMac": universal_macho(),
        prefix + "Contents/Resources/sq.version": metadata("osx-universal-stable", "Contents/MacOS/advanced-show-control"),
        prefix + "Contents/Info.plist": plistlib.dumps({"CFBundleIdentifier": "com.advancedshowcontrol.app", "CFBundleVersion": VERSION, "CFBundleShortVersionString": VERSION, "LSMinimumSystemVersion": "15.0"}),
        prefix + "Contents/_CodeSignature/CodeResources": "signature fixture",
    }


class PayloadTests(unittest.TestCase):
    def test_windows_update_and_portable_payloads(self):
        for portable in (False, True):
            payloads.validate_payload(zip_bytes(windows_files(portable)), "windows", VERSION, "win-x64-stable", portable)

    def test_windows_missing_updater_or_wrong_identity_is_rejected(self):
        files = windows_files()
        del files["lib/app/Squirrel.exe"]
        with self.assertRaises(ValueError):
            payloads.validate_payload(zip_bytes(files), "windows", VERSION, "win-x64-stable")
        files = windows_files()
        files["lib/app/sq.version"] = metadata("win-x64-nightly", "advanced-show-control.exe")
        with self.assertRaises(ValueError):
            payloads.validate_payload(zip_bytes(files), "windows", VERSION, "win-x64-stable")

    def test_windows_os_baseline_is_required_in_update_metadata(self):
        files = windows_files()
        files["lib/app/sq.version"] = files["lib/app/sq.version"].replace("10.0.15063", "6.1")
        with self.assertRaises(ValueError):
            payloads.validate_payload(zip_bytes(files), "windows", VERSION, "win-x64-stable")

    def test_macos_update_and_portable_payloads(self):
        for portable in (False, True):
            payloads.validate_payload(zip_bytes(macos_files(portable)), "macos", VERSION, "osx-universal-stable", portable)

    def test_macos_requires_universal_app_and_updater_and_retained_bundle_id(self):
        for name, replacement in [
            ("lib/app/Contents/MacOS/UpdateMac", b"not Mach-O"),
            ("lib/app/Contents/MacOS/advanced-show-control", struct.pack(">II", 0xCAFEBABE, 1) + struct.pack(">IIIII", 0x01000007, 0, 0, 0, 0)),
            ("lib/app/Contents/Info.plist", plistlib.dumps({"CFBundleIdentifier": "wrong"})),
        ]:
            files = macos_files()
            files[name] = replacement
            with self.subTest(name=name), self.assertRaises(ValueError):
                payloads.validate_payload(zip_bytes(files), "macos", VERSION, "osx-universal-stable")

    def test_feed_checks_reference_names_versions_hashes_and_sizes(self):
        filename = "com.advancedshowcontrol.app-12.177.49536-win-x64-stable-full.nupkg"
        data = zip_bytes(windows_files())
        asset = {"PackageId": "com.advancedshowcontrol.app", "Version": VERSION, "Type": "Full", "FileName": filename,
                 "SHA1": hashlib.sha1(data).hexdigest().upper(), "SHA256": hashlib.sha256(data).hexdigest().upper(), "Size": len(data)}
        feed = {"Assets": [asset]}
        self.assertEqual(payloads.validate_feed(feed, {filename: data}, VERSION), [filename])
        for field, value in [("Version", "1.0.0"), ("PackageId", "wrong"), ("FileName", "missing.nupkg"), ("Size", 0), ("SHA256", "0" * 64)]:
            bad = json.loads(json.dumps(feed))
            bad["Assets"][0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                payloads.validate_feed(bad, {filename: data}, VERSION)


if __name__ == "__main__":
    unittest.main()
