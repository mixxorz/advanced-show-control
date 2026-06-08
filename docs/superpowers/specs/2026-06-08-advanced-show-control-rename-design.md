## Summary

Rename the app from LV1 Scene Fade Utility to Advanced Show Control as a full project metadata rename.

## Scope

- Update user-facing product text to Advanced Show Control.
- Update Rust package, library, and binary identifiers to `advanced-show-control` and `advanced_show_control`.
- Update Tauri metadata so the desktop app presents as Advanced Show Control.
- Update frontend package metadata, document title, and header text.
- Keep the workspace checkout directory unchanged.
- Do not add persisted data migration in this pass.

## Approach

Use `advanced-show-control` for package and binary names, and `advanced_show_control` for Rust crate imports. Update references in source, tests, package manifests, lockfiles, Tauri config, and docs that describe the current product name.

## Testing

Verify the rename through build and type checks:

- `cargo test --workspace`
- `npm run typecheck`
- `npm run build`

## Non-Goals

- Renaming the repository or checkout folder.
- Migrating previously stored app data paths.
- Changing app behavior, scene safety logic, LV1 integration, or UI layout.
