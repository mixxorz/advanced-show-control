# Build Number Release Versioning Design

## Goal

Use simple build-number release tags such as `v1`, `v2`, and `v3` for GitHub Releases and release assets.

## Release Tags

The release workflow triggers only from tags matching `v[0-9]+`. Tags such as `v0.1.0`, `1`, and `release-1` do not trigger releases.

The tag string is the release version. The workflow does not strip the leading `v` for display, changelog lookup, or asset naming.

## Changelog

`CHANGELOG.md` uses matching tag headings with dates:

```markdown
## [v1] - 2026-07-04
```

The release workflow validates that the pushed tag has a matching changelog section. Release notes are extracted from that section until the next `## [` heading.

## Release Assets

Release assets include the full tag string:

- `Advanced-Show-Control_v1_Windows_x64_Setup.zip`
- `Advanced-Show-Control_v1_macOS_universal.dmg`

The Windows zip contains only the NSIS setup executable. The macOS asset is the universal dmg.

## Testing Tags

After this work is merged into `main`, test tags may be created and pushed from `main` to validate the release workflow. Test tags still use the same `vN` format and require matching changelog sections.

## Out of Scope

The workflow does not enforce consecutive build numbers. It accepts any positive integer-like tag matching `v[0-9]+`.
