#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS packaging must run on macOS" >&2
  exit 1
fi

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

release_id="${1:-local}"
app_name="Advanced Show Control"
binary_name="advanced-show-control"
bundle_id="com.advancedshowcontrol.app"
app_dir="dist/release/macos/${app_name}.app"
contents_dir="${app_dir}/Contents"
macos_dir="${contents_dir}/MacOS"
resources_dir="${contents_dir}/Resources"

version="$(cargo metadata --manifest-path app/Cargo.toml --no-deps --format-version 1 \
  | python3 -c 'import json,sys; data=json.load(sys.stdin); print(next(p["version"] for p in data["packages"] if p["name"] == "advanced-show-control"))')"

cargo build --manifest-path app/Cargo.toml --release --target aarch64-apple-darwin --bin "$binary_name"
cargo build --manifest-path app/Cargo.toml --release --target x86_64-apple-darwin --bin "$binary_name"

rm -rf "$app_dir"
mkdir -p "$macos_dir" "$resources_dir"
lipo -create \
  "target/aarch64-apple-darwin/release/${binary_name}" \
  "target/x86_64-apple-darwin/release/${binary_name}" \
  -output "${macos_dir}/${binary_name}"
chmod 755 "${macos_dir}/${binary_name}"
cp app/icons/icon.icns "${resources_dir}/app.icns"

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
  <string>${version}</string>
  <key>CFBundleVersion</key>
  <string>${version}</string>
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
# An ad-hoc signature has no developer identity and keeps the universal app launchable on Apple silicon.
codesign --force --deep --sign - "$app_dir"

archive="dist/release/Advanced-Show-Control_${release_id}_macOS_universal.zip"
rm -f "$archive"
ditto -c -k --sequesterRsrc --keepParent "$app_dir" "$archive"
echo "$archive"
