import importlib.util
import unittest
from datetime import datetime
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "release_metadata", Path(__file__).resolve().parents[1] / "release-metadata.py"
)
metadata = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(metadata)


class ReleaseMetadataTests(unittest.TestCase):
    def test_fixed_utc_timestamps_and_component_boundaries(self):
        cases = [
            ("2020-01-01T00:00:00+00:00", "0.0.0"),
            ("2020-01-01T00:00:01+00:00", "0.0.1"),
            ("2020-01-01T18:12:15+00:00", "0.0.65535"),
            ("2020-01-01T18:12:16+00:00", "0.1.0"),
            ("2020-07-13T04:20:15+00:00", "0.255.65535"),
            ("2020-07-13T04:20:16+00:00", "1.0.0"),
            ("2026-10-01T00:00:00+00:00", "12.177.49536"),
            ("2156-02-07T06:28:15+00:00", "255.255.65535"),
        ]
        versions = []
        for timestamp, expected in cases:
            with self.subTest(timestamp=timestamp):
                actual = metadata.version_from_timestamp(datetime.fromisoformat(timestamp))
                self.assertEqual(actual, expected)
                versions.append(tuple(map(int, actual.split("."))))
        self.assertEqual(versions, sorted(set(versions)))

    def test_timestamp_requires_timezone_and_representable_epoch_seconds(self):
        for timestamp in ["2019-12-31T23:59:59+00:00", "2156-02-07T06:28:16+00:00", "2026-10-01T00:00:00"]:
            with self.subTest(timestamp=timestamp), self.assertRaises(ValueError):
                metadata.version_from_timestamp(datetime.fromisoformat(timestamp))
        self.assertEqual(metadata.version_from_timestamp(datetime.fromisoformat("2026-10-01T08:00:00+08:00")), "12.177.49536")

    def test_tags_choose_channels_not_version_precedence(self):
        timestamp = datetime.fromisoformat("2026-10-01T00:00:00+00:00")
        for release_id, track in [("local", "stable"), ("v1", "stable"), ("v999999", "stable"), ("nightly-2028-02-29", "nightly")]:
            with self.subTest(release_id=release_id):
                result = metadata.resolve_metadata(release_id, timestamp=timestamp)
                self.assertEqual(result["release_version"], "12.177.49536")
                self.assertEqual(result["msi_version"], "12.177.49536")
                self.assertEqual(result["windows_channel"], f"win-x64-{track}")
                self.assertEqual(result["macos_channel"], f"osx-universal-{track}")

    def test_invalid_tags(self):
        for release_id in ["", "v0", "v01", "V1", "v1.2.3", "v1\n", "../v1", "nightly-2026-02-29", "nightly-2026-1-01", "nightly-2026-10-01\n"]:
            with self.subTest(release_id=release_id), self.assertRaises(ValueError):
                metadata.resolve_metadata(release_id, release_version="12.177.49536")

    def test_common_explicit_version_is_preserved(self):
        self.assertEqual(metadata.resolve_metadata("v1", "255.255.65535")["release_version"], "255.255.65535")
        for version in ["1.2", "01.2.3", "1.2.3.4", "1.2.3-nightly", "256.0.0", "1.256.0", "1.2.65536", "-1.0.0", "1.2.3\n"]:
            with self.subTest(version=version), self.assertRaises(ValueError):
                metadata.resolve_metadata("local", release_version=version)


if __name__ == "__main__":
    unittest.main()
