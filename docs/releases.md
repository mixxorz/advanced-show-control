# Packaging and software updates

Advanced Show Control uses Velopack 1.2.161 for Windows and macOS installation layouts and updates.
The Rust SDK and packaging CLI must use the same pinned version. GPUI owns the update controls;
Velopack owns package verification and replacement.

## Release identity

GitHub tags remain `vN` for numbered releases and `nightly-YYYY-MM-DD` for prereleases. These tags
are embedded in the executable and remain visible in About.

Velopack and Windows Installer require numeric versions. `scripts/release-metadata.py` encodes
UTC build time as unsigned seconds since 2020-01-01 in an `8-bit.8-bit.16-bit` version. This fits
MSI's three-field limits and orders builds chronologically across stable and nightly releases.
The reusable artifact workflow resolves the version once and passes it to both platform builds.
A rebuild receives a new version; do not publish an older version as a newer update.

Platform channels are:

| Platform | Stable | Nightly |
| --- | --- | --- |
| Windows x64 | `win-x64-stable` | `win-x64-nightly` |
| macOS universal | `osx-universal-stable` | `osx-universal-nightly` |

The nightly preference selects the nightly channel, rather than merging stable and nightly feeds.
Disabling it waits for a newer stable build; the app does not downgrade automatically.

## GitHub assets

Each release must publish the Velopack `releases.<channel>.json` feeds and the `.nupkg` files they
reference, in addition to human-facing installers and ZIPs. Do not rename a `.nupkg` independently
of its feed. Both platform artifacts must use the same release version and appropriate channel.

Stable checks use `https://github.com/mixxorz/advanced-show-control/releases/latest/download`.
This avoids Velopack's GitHub source limit of ten recent releases, which could otherwise hide a
stable release behind frequent nightlies. Nightly checks use the GitHub source with prereleases
included and an explicit platform-nightly channel.

GitHub release publication is the update distribution step. No separate update server or embedded
GitHub token is required. An older release without Velopack feeds cannot supply updates.

## Application behavior

- **Automatically check for updates** defaults on: check at startup and every six hours.
- **Include nightly updates** defaults off.
- Manual checks remain available when automatic checks are off.
- Checks never download, and downloads never install.
- **Software Updates…** offers download, then a separate **Update and restart** confirmation.
- Restart uses Save / Don't Save / Cancel, disconnects LV1, and rechecks the session revision before
  handing installation to Velopack. A cancelled or failed save never starts installation.
- Startup auto-apply is disabled. Ordinary Quit does not install downloaded packages.
- The updater helper waits for the current process to exit before applying and restarting.

The workflow assumes one ASC instance per computer. Velopack's Windows replacement can forcibly
terminate remaining processes from its installation directory. Updating is therefore an explicit
operator decision to stop ASC, not a background installation during a show.

Development executables and pre-Velopack releases display that updates are unavailable and make no
update network request. Existing users must install a Velopack-packaged release once to enter the
update workflow. Settings and diagnostics retain their existing app-data location. Keep user-owned
show files outside the installation directory: Velopack replaces that directory during updates.

## Integrity and signing

Velopack verifies package size and feed hashes before replacement. These detect corrupt downloads;
they are not independent publisher authentication if the GitHub release and feed are compromised.
Release publication and GitHub repository access remain trust boundaries.

Windows distribution remains unsigned. macOS distribution remains ad-hoc signed, without Developer
ID signing or notarization. Operating-system security prompts still apply. Signing configuration
must cover the final Velopack-produced bundle, including its update helper and metadata.

## Verification

Python packaging tests exercise version encoding and release payload/feed agreement. Native Rust
checks cover settings, updater mailbox behavior, channel invalidation, projection, and session
admission. `make visual-test` covers the native views on macOS. Windows installer execution and a
real old-to-new update require native Windows acceptance; cross-platform Rust tests alone do not
prove that an MSI installs or that the external update helper replaces a packaged app correctly.
