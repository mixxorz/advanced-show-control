#!/usr/bin/env python3
"""Run the pinned Velopack CLI with project-local tooling and caches."""

import os
import subprocess
import sys
from pathlib import Path

VPK_VERSION = "1.2.161"
REPO_ROOT = Path(__file__).resolve().parents[2]


def run(arguments):
    tool_root = REPO_ROOT / "dist/tools/velopack" / VPK_VERSION
    tool_path = tool_root / "bin"
    env = os.environ.copy()
    directories = {
        "DOTNET_CLI_HOME": REPO_ROOT / "dist/tools/dotnet-home",
        "NUGET_PACKAGES": REPO_ROOT / "dist/tools/nuget/packages",
        "NUGET_HTTP_CACHE_PATH": REPO_ROOT / "dist/tools/nuget/http",
        "NUGET_PLUGINS_CACHE_PATH": REPO_ROOT / "dist/tools/nuget/plugins",
        "XDG_CACHE_HOME": REPO_ROOT / "dist/tools/cache",
        "TEMP": REPO_ROOT / "dist/tools/tmp",
        "TMP": REPO_ROOT / "dist/tools/tmp",
        "TMPDIR": REPO_ROOT / "dist/tools/tmp",
        "VELOPACK_TEMP": REPO_ROOT / "dist/tools/tmp/velopack",
    }
    for key, directory in directories.items():
        directory.mkdir(parents=True, exist_ok=True)
        env[key] = str(directory)
    env.update(DOTNET_NOLOGO="1", DOTNET_CLI_TELEMETRY_OPTOUT="1", DOTNET_ROLL_FORWARD="Major",
               DOTNET_GENERATE_ASPNET_CERTIFICATE="false", DOTNET_ADD_GLOBAL_TOOLS_TO_PATH="false")
    tool_root.mkdir(parents=True, exist_ok=True)
    (tool_root / "global.json").write_text(
        '{"sdk":{"version":"8.0.100","rollForward":"latestFeature"}}', encoding="utf-8"
    )
    config = tool_root / "NuGet.Config"
    config.write_text(
        '<?xml version="1.0" encoding="utf-8"?><configuration><packageSources><clear/>'
        '<add key="nuget.org" value="https://api.nuget.org/v3/index.json"/>'
        '</packageSources></configuration>', encoding="utf-8"
    )
    sdk = subprocess.check_output(["dotnet", "--version"], cwd=tool_root, env=env, text=True).strip()
    if not sdk.startswith("8."):
        raise RuntimeError("Velopack packaging requires the .NET 8 SDK")
    executable = tool_path / ("vpk.exe" if os.name == "nt" else "vpk")
    if not executable.exists():
        subprocess.run(
            ["dotnet", "tool", "install", "vpk", "--version", VPK_VERSION,
             "--tool-path", str(tool_path), "--configfile", str(config)],
            cwd=tool_root, env=env, check=True,
        )
    installed = subprocess.check_output(
        ["dotnet", "tool", "list", "--tool-path", str(tool_path)], cwd=tool_root, env=env, text=True
    )
    if not any(row.split()[:2] == ["vpk", VPK_VERSION] for row in installed.splitlines()):
        raise RuntimeError(f"Expected project-local vpk {VPK_VERSION}")
    subprocess.run([str(executable), *arguments], cwd=REPO_ROOT, env=env, check=True)


if __name__ == "__main__":
    try:
        run(sys.argv[1:])
    except (OSError, subprocess.CalledProcessError, RuntimeError) as error:
        print(f"Velopack command failed: {error}", file=sys.stderr)
        sys.exit(1)
