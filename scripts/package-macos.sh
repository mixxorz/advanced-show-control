#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS packaging must run on macOS with Python 3 and .NET 8 SDK" >&2
  exit 1
fi

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

release_id="${1:-local}"
metadata_args=(--release-id "$release_id")
if [[ -n "${2:-}" ]]; then metadata_args+=(--release-version "$2"); fi
metadata="$(python3 scripts/release-metadata.py "${metadata_args[@]}")"
release_version="$(printf '%s' "$metadata" | python3 -c 'import json,sys; print(json.load(sys.stdin)["release_version"])')"
channel="$(printf '%s' "$metadata" | python3 -c 'import json,sys; print(json.load(sys.stdin)["macos_channel"])')"
export ASC_RELEASE_ID="$release_id" ASC_RELEASE_VERSION="$release_version"
app_name="Advanced Show Control"
binary_name="advanced-show-control"
bundle_id="com.advancedshowcontrol.app"
build_dir="$repo_root/dist/build/macos"
pack_root="$repo_root/dist/pack/macos"
app_dir="${pack_root}/${app_name}.app"
contents_dir="${app_dir}/Contents"
macos_dir="${contents_dir}/MacOS"
resources_dir="${contents_dir}/Resources"
vpk_output="${pack_root}/releases"
stage_dir="$repo_root/dist/release/macos"

cargo build --manifest-path app/Cargo.toml --release --target aarch64-apple-darwin --target-dir "$build_dir" --bin "$binary_name"
cargo build --manifest-path app/Cargo.toml --release --target x86_64-apple-darwin --target-dir "$build_dir" --bin "$binary_name"

rm -rf "$pack_root" "$stage_dir"
mkdir -p "$macos_dir" "$resources_dir"
lipo -create \
  "${build_dir}/aarch64-apple-darwin/release/${binary_name}" \
  "${build_dir}/x86_64-apple-darwin/release/${binary_name}" \
  -output "${macos_dir}/${binary_name}"
chmod 755 "${macos_dir}/${binary_name}"
cp app/icons/icon.icns "${resources_dir}/app.icns"
cp LICENSE "${resources_dir}/LICENSE.txt"

cat > "${contents_dir}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>${app_name}</string>
  <key>CFBundleExecutable</key>
  <string>${binary_name}</string>
  <key>CFBundleIconFile</key>
  <string>app.icns</string>
  <key>CFBundleIdentifier</key>
  <string>${bundle_id}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${app_name}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${release_version}</string>
  <key>CFBundleVersion</key>
  <string>${release_version}</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.utilities</string>
  <key>LSMinimumSystemVersion</key>
  <string>15.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSLocalNetworkUsageDescription</key>
  <string>Advanced Show Control discovers and controls an LV1 console on your local network.</string>
</dict>
</plist>
PLIST

plutil -lint "${contents_dir}/Info.plist"
# Velopack's default macOS RID is architecture-neutral; explicit osx-universal is unsupported.
# It signs the entire copied bundle after adding UpdateMac and sq.version, before making both archives.
python3 scripts/installer/velopack.py pack --channel "$channel" --noInst \
  --packId "$bundle_id" --packTitle "$app_name" --bundleId "$bundle_id" \
  --mainExe "$binary_name" --packVersion "$release_version" --packAuthors "$app_name" \
  --packDir "$app_dir" --outputDir "$vpk_output" --icon "$repo_root/app/icons/icon.icns" \
  --signAppIdentity - --signEntitlements "$repo_root/scripts/installer/macos.entitlements"
python3 scripts/installer/validate-release.py --directory "$vpk_output" --platform macos \
  --release-id "$release_id" --release-version "$release_version" --stage "$stage_dir"

archive="${stage_dir}/Advanced-Show-Control_${release_id}_macOS_universal.zip"
python3 scripts/tests/macos-release-smoke.py --archive "$archive" \
  --package "${stage_dir}/${bundle_id}-${release_version}-${channel}-full.nupkg" \
  --release-version "$release_version" --channel "$channel" --work-dir "${pack_root}/verify"
echo "macOS release $release_version staged at $stage_dir"
