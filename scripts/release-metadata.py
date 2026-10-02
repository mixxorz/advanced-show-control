#!/usr/bin/env python3
"""Shared build-time version and platform channels for release packaging."""

import argparse
import json
import re
from datetime import date, datetime, timedelta, timezone

EPOCH = datetime(2020, 1, 1, tzinfo=timezone.utc)


def version_from_timestamp(timestamp):
    if timestamp.tzinfo is None or timestamp.utcoffset() is None:
        raise ValueError("Release timestamp must include a timezone")
    seconds = (timestamp.astimezone(timezone.utc) - EPOCH) // timedelta(seconds=1)
    if not 0 <= seconds <= 0xFFFFFFFF:
        raise ValueError("Release timestamp must fit unsigned 32-bit seconds since 2020-01-01 UTC")
    return f"{seconds >> 24}.{(seconds >> 16) & 255}.{seconds & 65535}"


def validate_version(version):
    if not re.fullmatch(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", version):
        raise ValueError("Release version must be a canonical three-component numeric SemVer")
    major, minor, patch = map(int, version.split("."))
    if major > 255 or minor > 255 or patch > 65535:
        raise ValueError("Release version exceeds MSI's 255.255.65535 limits")
    return version


def release_track(release_id):
    if release_id == "local" or re.fullmatch(r"v[1-9][0-9]*", release_id):
        return "stable"
    if re.fullmatch(r"nightly-[0-9]{4}-[0-9]{2}-[0-9]{2}", release_id):
        try:
            date.fromisoformat(release_id.removeprefix("nightly-"))
            return "nightly"
        except ValueError:
            pass
    raise ValueError("Release ID must be local, a numbered tag like v1, or nightly-YYYY-MM-DD")


def resolve_metadata(release_id, release_version=None, timestamp=None):
    track = release_track(release_id)
    version = validate_version(release_version) if release_version else version_from_timestamp(
        timestamp if timestamp is not None else datetime.now(timezone.utc)
    )
    return {
        "release_id": release_id,
        "release_version": version,
        "msi_version": version,
        "track": track,
        "windows_channel": f"win-x64-{track}",
        "macos_channel": f"osx-universal-{track}",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release-id", default="local")
    parser.add_argument("--release-version")
    parser.add_argument("--timestamp", help="ISO 8601 build timestamp; defaults to current UTC time")
    parser.add_argument("--format", choices=("json", "version", "github"), default="json")
    args = parser.parse_args()
    try:
        timestamp = datetime.fromisoformat(args.timestamp.replace("Z", "+00:00")) if args.timestamp else None
        result = resolve_metadata(args.release_id, args.release_version, timestamp)
    except ValueError as error:
        parser.error(str(error))
    if args.format == "json":
        print(json.dumps(result))
    elif args.format == "version":
        print(result["release_version"])
    else:
        for key, value in result.items():
            print(f"{key}={value}")


if __name__ == "__main__":
    main()
