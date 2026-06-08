# Advanced Show Control Rename Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the app from LV1 Scene Fade Utility to Advanced Show Control across user-facing text, build metadata, package names, binaries, and Rust crate imports.

**Architecture:** This is a metadata and identifier rename, not a behavior change. The source tree stays in the existing checkout directory while Cargo, Tauri, npm, docs, UI text, and Rust imports move to `advanced-show-control` / `advanced_show_control`.

**Tech Stack:** Rust/Cargo workspace, Tauri 2, React/TypeScript/Vite, npm package metadata.

---

## File Structure

- Modify `Cargo.toml`: rename root Cargo package and library crate.
- Modify `src-tauri/Cargo.toml`: rename Tauri Cargo package, description, and dependency key for the root crate.
- Modify `src-tauri/tauri.conf.json`: rename app product and identifier/bundle metadata to the new product identity.
- Modify Rust source and tests under `src/`, `src-tauri/src/`, and `tests/`: replace imports and direct paths from `lv1_scene_fade_utility` to `advanced_show_control`.
- Modify `package.json`, `ui/package.json`, `package-lock.json`, and `ui/package-lock.json`: rename npm packages to `advanced-show-control`.
- Modify `ui/src/components/Header.tsx`: update visible app header.
- Modify frontend document title source, likely `ui/index.html` if present or generated/dist files if that is the current source checked in.
- Modify `README.md` and relevant project docs: update current product name while preserving LV1 integration descriptions.
- Modify `Cargo.lock`: regenerate by running Cargo after manifest edits.

### Task 1: Rename Cargo Workspace Metadata

**Files:**
- Modify: `Cargo.toml`
- Modify: `src-tauri/Cargo.toml`
- Modify: `Cargo.lock`

- [ ] **Step 1: Update root Cargo manifest**

Change `Cargo.toml` package and library names:

```toml
[package]
name = "advanced-show-control"
version = "0.1.0"
edition = "2024"
license = "GPL-3.0-or-later"

[workspace]
members = [".", "src-tauri"]
resolver = "2"

[lib]
name = "advanced_show_control"
path = "src/lib.rs"
```

- [ ] **Step 2: Update Tauri Cargo manifest**

Change `src-tauri/Cargo.toml` package metadata and root dependency key:

```toml
[package]
name = "advanced-show-control-tauri"
version = "0.1.0"
description = "Desktop shell for Advanced Show Control"
authors = ["you"]
edition = "2024"
license = "GPL-3.0-or-later"

[dependencies]
advanced-show-control = { path = ".." }
```

Preserve all other existing dependency entries unchanged.

- [ ] **Step 3: Regenerate Cargo lockfile**

Run: `cargo metadata --format-version 1`

Expected: command exits successfully and `Cargo.lock` package entries use `advanced-show-control` and `advanced-show-control-tauri`.

### Task 2: Rename Rust Crate Imports

**Files:**
- Modify: `src/main.rs`
- Modify: `src-tauri/src/**/*.rs`
- Modify: `tests/*.rs`

- [ ] **Step 1: Replace crate paths in source and tests**

Replace every Rust path prefix:

```rust
lv1_scene_fade_utility::
```

with:

```rust
advanced_show_control::
```

- [ ] **Step 2: Replace Cargo dependency imports in Tauri code**

If any `use lv1_scene_fade_utility::...` remains under `src-tauri/src`, replace it with:

```rust
use advanced_show_control::...;
```

- [ ] **Step 3: Verify no old Rust crate identifier remains in source**

Run: `rg "lv1_scene_fade_utility" src src-tauri/src tests`

Expected: no matches.

### Task 3: Rename Tauri And Frontend Product Metadata

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `package.json`
- Modify: `ui/package.json`
- Modify: `package-lock.json`
- Modify: `ui/package-lock.json`

- [ ] **Step 1: Update Tauri product metadata**

Set the product name to:

```json
"productName": "Advanced Show Control"
```

Set the app identifier to a reverse-DNS value based on the new name, for example:

```json
"identifier": "com.advancedshowcontrol.app"
```

Preserve unrelated Tauri configuration unchanged.

- [ ] **Step 2: Update npm package names**

Set both root and UI package names to:

```json
"name": "advanced-show-control"
```

If lockfiles contain package metadata with the old name, update those package entries to `advanced-show-control` while preserving dependency versions.

### Task 4: Rename User-Facing Text

**Files:**
- Modify: `README.md`
- Modify: `ui/src/components/Header.tsx`
- Modify: frontend HTML title source, likely `ui/index.html` if present
- Modify: project docs with current app title where appropriate

- [ ] **Step 1: Update UI header**

Change the header title to:

```tsx
<h1 className="text-xl font-semibold">Advanced Show Control</h1>
```

- [ ] **Step 2: Update document title**

Change the frontend HTML title to:

```html
<title>Advanced Show Control</title>
```

- [ ] **Step 3: Update README product name**

Change the README heading to:

```markdown
# Advanced Show Control
```

Change the license sentence to:

```markdown
Advanced Show Control is licensed under the GNU General Public License version 3 or later. See [LICENSE](LICENSE) for details.
```

- [ ] **Step 4: Preserve LV1 domain wording**

Keep references to Waves eMotion LV1, LV1 scenes, and LV1 scene workflows. Only rename the app/product, not the domain concepts.

### Task 5: Verify Rename

**Files:**
- No intended source edits unless verification exposes missed rename references.

- [ ] **Step 1: Search for old product and crate names**

Run: `rg "LV1 Scene Fade Utility|Scene Fade Utility|lv1-scene-fade-utility|lv1_scene_fade_utility" --glob '!target/**' --glob '!ui/dist/**' --glob '!node_modules/**'`

Expected: no matches except historical design/spec documents where preserving old wording is intentional.

- [ ] **Step 2: Run Rust tests**

Run: `cargo test --workspace`

Expected: all workspace tests pass.

- [ ] **Step 3: Run frontend typecheck**

Run: `npm run typecheck`

Expected: TypeScript typecheck passes.

- [ ] **Step 4: Run frontend build**

Run: `npm run build`

Expected: production build completes and generated title/assets reflect Advanced Show Control.

## Self-Review

- Spec coverage: all product text, Cargo identifiers, Tauri metadata, npm metadata, docs, and verification requirements are covered.
- Placeholder scan: no TBD/TODO/fill-in steps remain.
- Type consistency: the plan consistently uses `advanced-show-control` for package/binary naming and `advanced_show_control` for Rust crate imports.
