# Native Session Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move session file actions to the native File menu, remove the Sessions tab, show session state in the window title, and rename saved session files to `.ascs` without changing the JSON schema.

**Architecture:** Keep LV1 connection controls in the React app. Add native menu handling in the Tauri adapter layer and have menu handlers send the same `ShowCommand` mailbox messages as the existing command adapters. Update the React runtime title from projected `AppViewState` using Tauri's window API.

**Tech Stack:** Rust/Tauri 2, `rfd` native dialogs, Tokio actor mailboxes, React 19, TypeScript, Vitest, Cargo nextest.

## Global Constraints

- Do not add an in-app session dropdown or top-bar session control.
- Do not change the show/session JSON schema.
- Do not add old extension migration or compatibility support.
- Do not extract shared functions for this work; small repeated adapter code is preferred over extra indirection.
- Native File menu actions must use: `New Session`, `Open Session...`, `Save Session`, `Save As...`.
- `.ascs` is the preferred extension for dialogs, defaults, backups, and tests.
- Window title format is `Advanced Show Control - <session name without extension>` plus ` *` when dirty.
- The React title update API is `getCurrentWindow().setTitle(title)` from `@tauri-apps/api/window`.
- Preserve existing safety behavior, actor mailbox boundaries, and projector-owned `AppViewState` projection.

---

## File Structure

- Modify `ui/src/components/TopTabBar.tsx`: remove `sessions` from `MainTab` and the top tab list.
- Modify `ui/src/components/AppShell.tsx`: remove `SessionsTab` import and rendering branch.
- Delete `ui/src/components/SessionsTab.tsx` and `ui/src/components/SessionsTab.stories.tsx` if no imports remain.
- Modify `ui/src/components/TopTabBar.test.tsx`, `ui/src/components/AppShell.test.tsx`, and Storybook fixture files that reference `sessions`.
- Create `ui/src/sessionTitle.ts`: pure title formatter for testing.
- Modify `ui/src/AppRuntime.tsx`: accept an optional title setter service and update the window title from projected show/session fields.
- Modify `ui/src/App.tsx`: wire `getCurrentWindow().setTitle(title)` into `AppRuntimeServices`.
- Modify `src-tauri/src/show_file.rs`: rename extension-sensitive backup matching and backup filenames to `.ascs`.
- Modify `src-tauri/src/ui/commands/show.rs`: update dialog filters, default filenames, and user-facing cancellation strings to session terminology and `.ascs`.
- Create `src-tauri/src/ui/menu.rs`: native File menu construction and menu event handling.
- Modify `src-tauri/src/ui/mod.rs`: register the native menu and handler in `build_app`.
- Modify `docs/roadmap.md`: remove completed/obsolete Sessions-tab roadmap language and mark the extension rename direction as settled if needed.

---

### Task 1: Remove Sessions Tab

**Files:**
- Modify: `ui/src/components/TopTabBar.tsx`
- Modify: `ui/src/components/AppShell.tsx`
- Modify: `ui/src/components/TopTabBar.test.tsx`
- Modify: `ui/src/components/AppShell.test.tsx`
- Delete: `ui/src/components/SessionsTab.tsx`
- Delete: `ui/src/components/SessionsTab.stories.tsx`

**Interfaces:**
- Consumes: existing `MainTab` string union in `TopTabBar.tsx`.
- Produces: `MainTab = "scenes" | "playlists" | "events" | "logs" | "settings"` for later tasks.

- [ ] **Step 1: Write failing tests for removed Sessions tab**

In `ui/src/components/TopTabBar.test.tsx`, add:

```tsx
it("does not render a Sessions tab", () => {
  renderTopBar(connectedAppState);

  expect(screen.queryByRole("button", { name: "Sessions" })).not.toBeInTheDocument();
});
```

In `ui/src/components/AppShell.test.tsx`, add:

```tsx
it("does not render the Sessions tab in the shell navigation", () => {
  renderWithAppProviders(
    <AppShell
      activeTab="scenes"
      onOpenConnection={vi.fn()}
      onResume={vi.fn()}
      onSelectTab={vi.fn()}
      showConnection={false}
    />,
    { appState: connectedAppState },
  );

  expect(screen.queryByRole("button", { name: "Sessions" })).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run focused frontend tests and verify failure**

Run: `npm run test -- --run ui/src/components/TopTabBar.test.tsx ui/src/components/AppShell.test.tsx`

Expected: FAIL because `Sessions` is still rendered.

- [ ] **Step 3: Remove Sessions from top-level shell**

In `ui/src/components/TopTabBar.tsx`, replace the `MainTab` union and `tabs` array with:

```tsx
export type MainTab = "scenes" | "playlists" | "events" | "logs" | "settings";

const tabs: { id: MainTab; label: string }[] = [
  { id: "scenes", label: "Scenes" },
  { id: "playlists", label: "Cue Lists" },
  { id: "events", label: "Events" },
  { id: "logs", label: "Logs" },
  { id: "settings", label: "Settings" },
];
```

In `ui/src/components/AppShell.tsx`, remove `import { SessionsTab } from "./SessionsTab";` and delete this branch:

```tsx
{props.activeTab === "sessions" && <SessionsTab />}
```

Delete `ui/src/components/SessionsTab.tsx` and `ui/src/components/SessionsTab.stories.tsx`.

- [ ] **Step 4: Run focused frontend tests and typecheck**

Run: `npm run test -- --run ui/src/components/TopTabBar.test.tsx ui/src/components/AppShell.test.tsx`

Expected: PASS.

Run: `npm run typecheck`

Expected: PASS or FAIL only on remaining references to `sessions`; remove those references from stories/fixtures/tests and rerun until PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/TopTabBar.tsx ui/src/components/AppShell.tsx ui/src/components/TopTabBar.test.tsx ui/src/components/AppShell.test.tsx ui/src/components/SessionsTab.tsx ui/src/components/SessionsTab.stories.tsx
git commit -m "feat: remove sessions tab"
```

---

### Task 2: Rename Session File Extension To `.ascs`

**Files:**
- Modify: `src-tauri/src/show_file.rs`
- Modify: `src-tauri/src/ui/commands/show.rs`

**Interfaces:**
- Consumes: existing `read_show_file`, `write_show_file`, backup helpers, and Tauri show commands.
- Produces: `.ascs` dialog filters/default filenames and `.ascs` backup filenames.

- [ ] **Step 1: Write failing Rust expectations for `.ascs` backups**

In `src-tauri/src/show_file.rs`, update extension-sensitive tests to expect `.ascs`. Example replacements:

```rust
let show_path = temp_dir.join("test.ascs");
let candidate = backup_dir.join("123-test.ascs");
reserve_unique_backup_file(&backup_dir, Path::new("test.ascs"), "123").unwrap();
Some("123-test__backup1.ascs")
```

For pruning tests, replace `.lv1show` filenames with `.ascs`, including assertions such as:

```rust
assert!(is_backup_for_show_file("100-mix.ascs", "mix"));
assert!(!is_backup_for_show_file("101-mix-1.ascs", "mix"));
assert!(is_backup_for_show_file("101-mix-1.ascs", "mix-1"));
```

Keep JSON serialization tests unchanged; the schema is not changing.

- [ ] **Step 2: Run focused Rust tests and verify failure**

Run: `cargo nextest run -p advanced-show-control show_file`

Expected: FAIL because implementation still creates and matches `.lv1show` backup files.

- [ ] **Step 3: Update backup extension implementation**

In `src-tauri/src/show_file.rs`, change extension-specific backup code:

```rust
fn is_backup_for_show_file(name: &str, stem: &str) -> bool {
    let Some(prefix) = name.strip_suffix(".ascs") else {
        return false;
    };

    let Some((_, source)) = prefix.split_once('-') else {
        return false;
    };

    source == stem || source.starts_with(&format!("{stem}__backup"))
}
```

And:

```rust
reserve_unique_file(backup_dir, |suffix| {
    if suffix == 0 {
        format!("{timestamp}-{stem}.ascs")
    } else {
        format!("{timestamp}-{stem}__backup{suffix}.ascs")
    }
})
```

- [ ] **Step 4: Update native dialogs and copy**

In `src-tauri/src/ui/commands/show.rs`, change dialog filters/defaults:

```rust
.add_filter("Advanced Show Control Session", &["adsc"])
```

```rust
.set_file_name("Untitled.ascs")
```

Change cancellation strings:

```rust
.ok_or_else(|| "Open session cancelled".to_string())?;
.ok_or_else(|| "Save session cancelled".to_string())?;
```

- [ ] **Step 5: Run focused Rust tests**

Run: `cargo nextest run -p advanced-show-control show_file`

Expected: PASS.

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/show_file.rs src-tauri/src/ui/commands/show.rs
git commit -m "feat: use adsc session files"
```

---

### Task 3: Add Projection-Driven Window Title

**Files:**
- Create: `ui/src/sessionTitle.ts`
- Create or modify: `ui/src/sessionTitle.test.ts`
- Modify: `ui/src/AppRuntime.tsx`
- Modify: `ui/src/AppRuntime.test.tsx`
- Modify: `ui/src/App.tsx`

**Interfaces:**
- Produces: `formatSessionWindowTitle(showFileName: string, dirty: boolean): string`.
- Produces: `AppRuntimeServices.setWindowTitle?: (title: string) => Promise<unknown> | void`.
- Consumes: `AppViewState.showFileName` and `AppViewState.showFileDirty`.

- [ ] **Step 1: Write failing title formatter tests**

Create `ui/src/sessionTitle.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { formatSessionWindowTitle } from "./sessionTitle";

describe("formatSessionWindowTitle", () => {
  it("formats an untitled clean session", () => {
    expect(formatSessionWindowTitle("Untitled", false)).toBe(
      "Advanced Show Control - Untitled",
    );
  });

  it("adds a dirty marker", () => {
    expect(formatSessionWindowTitle("Tour Prep.ascs", true)).toBe(
      "Advanced Show Control - Tour Prep.ascs *",
    );
  });
});
```

- [ ] **Step 2: Run formatter test and verify failure**

Run: `npm run test -- --run ui/src/sessionTitle.test.ts`

Expected: FAIL because `sessionTitle.ts` does not exist.

- [ ] **Step 3: Implement formatter**

Create `ui/src/sessionTitle.ts`:

```ts
const APP_TITLE = "Advanced Show Control";

export function formatSessionWindowTitle(
  showFileName: string,
  dirty: boolean,
) {
  return `${APP_TITLE} - ${showFileName}${dirty ? " *" : ""}`;
}
```

- [ ] **Step 4: Add runtime title service test**

In `ui/src/AppRuntime.test.tsx`, add:

```tsx
it("updates the window title from projected session state", async () => {
  const setWindowTitle = vi.fn(async () => undefined);
  const services = makeServices({ setWindowTitle });

  render(<AppRuntime services={services} />);

  await waitFor(() => {
    expect(setWindowTitle).toHaveBeenCalledWith(
      "Advanced Show Control - Untitled Show *",
    );
  });
});
```

If `connectedAppState.showFileName` differs, use the exact value from `ui/src/storybook/mockAppState.ts` in the expected title.

- [ ] **Step 5: Implement title service in runtime**

In `ui/src/AppRuntime.tsx`, import the formatter:

```tsx
import { formatSessionWindowTitle } from "./sessionTitle";
```

Add to `AppRuntimeServices`:

```ts
setWindowTitle?: (title: string) => Promise<unknown> | void;
```

Add an effect after `appState` is declared:

```tsx
useEffect(() => {
  const title = formatSessionWindowTitle(
    appState.showFileName,
    appState.showFileDirty,
  );
  void Promise.resolve(services.setWindowTitle?.(title)).catch((error) => {
    setCommandError(String(error));
  });
}, [appState.showFileDirty, appState.showFileName, services]);
```

In `ui/src/App.tsx`, add:

```tsx
import { getCurrentWindow } from "@tauri-apps/api/window";
```

And add to `services`:

```ts
setWindowTitle: (title) => getCurrentWindow().setTitle(title),
```

- [ ] **Step 6: Run focused frontend tests**

Run: `npm run test -- --run ui/src/sessionTitle.test.ts ui/src/AppRuntime.test.tsx`

Expected: PASS.

Run: `npm run typecheck`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add ui/src/sessionTitle.ts ui/src/sessionTitle.test.ts ui/src/AppRuntime.tsx ui/src/AppRuntime.test.tsx ui/src/App.tsx
git commit -m "feat: update title with session name"
```

---

### Task 4: Add Native File Menu

**Files:**
- Create: `src-tauri/src/ui/menu.rs`
- Modify: `src-tauri/src/ui/mod.rs`
- Modify: `src-tauri/src/ui/commands/show.rs` only if public visibility changes are needed for command reuse; do not extract shared helpers.

**Interfaces:**
- Produces constants: `MENU_NEW_SESSION`, `MENU_OPEN_SESSION`, `MENU_SAVE_SESSION`, `MENU_SAVE_SESSION_AS`.
- Produces function: `pub fn install_session_menu(app: &mut tauri::App<tauri::Wry>) -> tauri::Result<()>`.
- Produces function: `pub fn handle_session_menu_event(app: &tauri::AppHandle<tauri::Wry>, event: tauri::menu::MenuEvent)`.

- [ ] **Step 1: Write menu construction tests**

Create test-only assertions in `src-tauri/src/ui/menu.rs` after adding the module skeleton:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_are_stable() {
        assert_eq!(MENU_NEW_SESSION, "session:new");
        assert_eq!(MENU_OPEN_SESSION, "session:open");
        assert_eq!(MENU_SAVE_SESSION, "session:save");
        assert_eq!(MENU_SAVE_SESSION_AS, "session:save-as");
    }
}
```

- [ ] **Step 2: Run Rust test and verify failure**

Run: `cargo nextest run -p advanced-show-control ui::menu`

Expected: FAIL because `ui::menu` does not exist.

- [ ] **Step 3: Implement menu constants and module registration**

Create `src-tauri/src/ui/menu.rs` with:

```rust
use crate::lifecycle::AppLifecycle;
use crate::runtime::errors::AppCommandError;
use crate::show::{ShowCommand, ShowCommandResult};
use crate::show_file::default_show_folder;
use std::path::PathBuf;
use tauri::menu::{Menu, MenuEvent, MenuItem, Submenu};
use tauri::{App, AppHandle, Manager, Runtime};
use tokio::sync::oneshot;
use tokio::task::spawn_blocking;

pub const MENU_NEW_SESSION: &str = "session:new";
pub const MENU_OPEN_SESSION: &str = "session:open";
pub const MENU_SAVE_SESSION: &str = "session:save";
pub const MENU_SAVE_SESSION_AS: &str = "session:save-as";

pub fn install_session_menu(app: &mut App<tauri::Wry>) -> tauri::Result<()> {
    let handle = app.handle();
    let file_menu = Submenu::with_items(
        handle,
        "File",
        true,
        &[
            &MenuItem::with_id(handle, MENU_NEW_SESSION, "New Session", true, None::<&str>)?,
            &MenuItem::with_id(handle, MENU_OPEN_SESSION, "Open Session...", true, None::<&str>)?,
            &MenuItem::with_id(handle, MENU_SAVE_SESSION, "Save Session", true, None::<&str>)?,
            &MenuItem::with_id(handle, MENU_SAVE_SESSION_AS, "Save As...", true, None::<&str>)?,
        ],
    )?;
    let menu = Menu::with_items(handle, &[&file_menu])?;
    app.set_menu(menu)?;
    Ok(())
}
```

In `src-tauri/src/ui/mod.rs`, add:

```rust
mod menu;
```

And inside `.setup(|app| { ... })`, after managed state setup:

```rust
menu::install_session_menu(app)?;
```

- [ ] **Step 4: Implement menu event handler without helper extraction**

In `src-tauri/src/ui/menu.rs`, add direct mailbox-sending functions. Keep each function explicit even if there is repetition:

```rust
pub fn handle_session_menu_event(app: &AppHandle<tauri::Wry>, event: MenuEvent) {
    let id = event.id().as_ref();
    let app = app.clone();
    match id {
        MENU_NEW_SESSION => tauri::async_runtime::spawn(async move {
            if let Err(err) = new_session_from_menu(app).await {
                tracing::warn!(error = %err, "New Session menu command failed");
            }
        }),
        MENU_OPEN_SESSION => tauri::async_runtime::spawn(async move {
            if let Err(err) = open_session_from_menu(app).await {
                tracing::warn!(error = %err, "Open Session menu command failed");
            }
        }),
        MENU_SAVE_SESSION => tauri::async_runtime::spawn(async move {
            if let Err(err) = save_session_from_menu(app).await {
                tracing::warn!(error = %err, "Save Session menu command failed");
            }
        }),
        MENU_SAVE_SESSION_AS => tauri::async_runtime::spawn(async move {
            if let Err(err) = save_session_as_from_menu(app).await {
                tracing::warn!(error = %err, "Save As menu command failed");
            }
        }),
        _ => return,
    };
}
```

Implement each async function explicitly, using the same pattern as `src-tauri/src/ui/commands/show.rs`: obtain `AppLifecycle` with `app.state::<AppLifecycle>()`, get `current_show().await`, create a oneshot reply, send the matching `ShowCommand`, and await the reply. For `Open Session...`, `Save Session`, and `Save As...`, use `rfd::FileDialog` with `.add_filter("Advanced Show Control Session", &["ascs"])` and `.set_file_name("Untitled.ascs")` where saving requires a default.

- [ ] **Step 5: Wire menu events in Tauri builder**

In `src-tauri/src/ui/mod.rs`, add builder menu event registration before `.setup(...)` or after it in the builder chain:

```rust
.on_menu_event(|app, event| {
    menu::handle_session_menu_event(app, event);
})
```

- [ ] **Step 6: Run Rust checks**

Run: `cargo fmt --all -- --check`

Expected: PASS.

Run: `cargo nextest run -p advanced-show-control ui::menu`

Expected: PASS.

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/ui/menu.rs src-tauri/src/ui/mod.rs src-tauri/src/ui/commands/show.rs
git commit -m "feat: add native session file menu"
```

---

### Task 5: Final Verification And Docs Alignment

**Files:**
- Modify: `docs/roadmap.md`
- Modify: any snapshots/stories/tests surfaced by verification.

**Interfaces:**
- Consumes: all previous task changes.
- Produces: verified branch with docs matching behavior.

- [ ] **Step 1: Update roadmap language**

In `docs/roadmap.md`, update MVP roadmap items that still describe a future Sessions tab. Replace item 12 with settled behavior, for example:

```md
12. Use native session file management.
    - Manage app session files through the native File menu.
    - Use `.ascs` as the app-owned session file extension.
    - Show current session and dirty state in the window title.
```

Update exit criteria lines that mention `Sessions` tab/import/export so they describe native session file management instead.

- [ ] **Step 2: Run full relevant verification**

Run: `make fmt`

Expected: PASS.

Run: `make lint`

Expected: PASS.

Run: `make test`

Expected: PASS.

Run: `make build`

Expected: PASS.

- [ ] **Step 3: Inspect final diff**

Run: `git status --short`

Expected: only intended files changed.

Run: `git diff --stat`

Expected: changes are limited to session menu, extension rename, title update, tab removal, and docs.

- [ ] **Step 4: Commit docs and any verification fixes**

```bash
git add docs/roadmap.md
git add ui src-tauri
git commit -m "docs: align roadmap with native sessions"
```

Skip the commit if there are no changes after verification.

---

## Self-Review

- Spec coverage: Tasks cover Sessions tab removal, native File menu, title bar updates, `.ascs` extension rename, no schema change, no compatibility layer, and no in-app dropdown.
- Placeholder scan: No `TBD`, `TODO`, or unspecified edge handling remains.
- Type consistency: `MainTab`, `formatSessionWindowTitle`, `setWindowTitle`, and menu command IDs are named consistently across tasks.
