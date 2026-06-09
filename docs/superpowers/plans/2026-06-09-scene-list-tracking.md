# Scene List Tracking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve app scene fade configs across deterministic LV1 scene renames, moves, inserts, and deletes while keeping exact recall validation.

**Architecture:** `Lv1Actor` already emits `SceneListChanged(Vec<SceneListEntry>)`; leave it factual. Implement scene-list classification and config transforms inside `ShowState::reconcile_scene_fade_configs`. Add a React-only persistent warning in `SceneTab` when duplicate scene names make tracking harder.

**Tech Stack:** Rust core crate, Tauri shell projection, React/TypeScript frontend, Cargo unit tests, npm typecheck/build.

---

## File Structure

- Modify `src/show/state.rs`: add private scene-list classification helpers, transform reconciliation behavior, and focused unit tests.
- Modify `ui/src/components/SceneTab.tsx`: derive duplicate scene-name warning from `appState.sceneConfigs` and render it above the scene list.
- Modify `docs/scene-tracking.md`: update after implementation if behavior differs from the design.
- Do not modify `src/lv1/*` for classification. `Lv1Actor` remains a fact publisher.
- Do not add a new durable scene ID.

---

### Task 1: Add Rename Reconciliation

**Files:**
- Modify: `src/show/state.rs`

- [ ] **Step 1: Write failing rename test**

Add this test inside `#[cfg(test)] mod tests` in `src/show/state.rs`:

```rust
#[test]
fn reconciliation_tracks_single_scene_rename() {
    let mut state = ShowState {
        lockout: false,
        scene_configs: vec![scene_config(
            "1::Verse",
            1_500,
            vec![ChannelConfig {
                group: 0,
                channel: 1,
                fader_db: Some(-12.0),
            }],
        )],
    };

    assert!(state.reconcile_scene_fade_configs(&[SceneListEntry {
        index: 1,
        name: "Verse Big".to_string(),
    }]));

    assert_eq!(state.scene_configs.len(), 1);
    assert_eq!(state.scene_configs[0].scene_id, "1::Verse Big");
    assert_eq!(state.scene_configs[0].scene_index, 1);
    assert_eq!(state.scene_configs[0].scene_name, "Verse Big");
    assert_eq!(state.scene_configs[0].duration_ms, 1_500);
    assert_eq!(state.scene_configs[0].channel_configs[0].fader_db, Some(-12.0));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p advanced-show-control show::state::tests::reconciliation_tracks_single_scene_rename`

Expected: FAIL because current exact-match reconciliation creates a default config and loses the stored channel config.

- [ ] **Step 3: Implement minimal rename support**

In `src/show/state.rs`, add these private helpers above `impl ShowState`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
struct SceneEntry {
    index: i32,
    name: String,
}

fn entries_from_configs(configs: &[SceneConfig]) -> Vec<SceneEntry> {
    let mut entries: Vec<_> = configs
        .iter()
        .map(|scene| SceneEntry {
            index: scene.scene_index,
            name: scene.scene_name.clone(),
        })
        .collect();
    entries.sort_by_key(|entry| entry.index);
    entries
}

fn entries_from_scene_list(scenes: &[SceneListEntry]) -> Vec<SceneEntry> {
    let mut entries: Vec<_> = scenes
        .iter()
        .map(|scene| SceneEntry {
            index: scene.index,
            name: scene.name.clone(),
        })
        .collect();
    entries.sort_by_key(|entry| entry.index);
    entries
}

fn default_scene_config(entry: &SceneEntry) -> SceneConfig {
    SceneConfig {
        scene_id: scene_id(entry.index, &entry.name),
        scene_index: entry.index,
        scene_name: entry.name.clone(),
        duration_ms: 0,
        channel_configs: Vec::new(),
        scoped_channels: Vec::new(),
        scope_toggles: SceneScopeToggles::default(),
    }
}

fn update_scene_locator(config: &mut SceneConfig, entry: &SceneEntry) {
    config.scene_id = scene_id(entry.index, &entry.name);
    config.scene_index = entry.index;
    config.scene_name = entry.name.clone();
}
```

Then replace the beginning of `reconcile_scene_fade_configs` with rename classification before the existing exact fallback:

```rust
pub fn reconcile_scene_fade_configs(&mut self, scenes: &[SceneListEntry]) -> bool {
    let old_entries = entries_from_configs(&self.scene_configs);
    let new_entries = entries_from_scene_list(scenes);

    if old_entries == new_entries {
        return false;
    }

    if old_entries.len() == new_entries.len() {
        let changed_at: Vec<_> = old_entries
            .iter()
            .zip(new_entries.iter())
            .enumerate()
            .filter(|(_, (old, new))| old != new)
            .collect();
        if changed_at.len() == 1 {
            let (_, (old, new)) = changed_at[0];
            if old.index == new.index {
                if let Some(scene) = self
                    .scene_configs
                    .iter_mut()
                    .find(|scene| scene.scene_index == old.index && scene.scene_name == old.name)
                {
                    update_scene_locator(scene, new);
                    self.scene_configs.sort_by_key(|scene| scene.scene_index);
                    return true;
                }
            }
        }
    }

    let mut next = Vec::with_capacity(scenes.len());
    for entry in &new_entries {
        let id = scene_id(entry.index, &entry.name);
        if let Some(existing) = self.scene_configs.iter().find(|scene| scene.scene_id == id) {
            next.push(existing.clone());
        } else {
            next.push(default_scene_config(entry));
        }
    }

    let changed = next != self.scene_configs;
    self.scene_configs = next;
    changed
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p advanced-show-control show::state::tests::reconciliation_tracks_single_scene_rename`

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/show/state.rs
git commit -m "test: cover scene rename reconciliation"
```

---

### Task 2: Add Move, Insert, Delete, And Ambiguous Fallback

**Files:**
- Modify: `src/show/state.rs`

- [ ] **Step 1: Write failing movement tests**

Add these tests to `src/show/state.rs`:

```rust
fn named_scene_config(index: i32, name: &str, duration_ms: u64) -> SceneConfig {
    scene_config(&format!("{index}::{name}"), duration_ms, vec![ChannelConfig {
        group: 0,
        channel: index,
        fader_db: Some(-10.0 - f64::from(index)),
    }])
}

fn scene_entry(index: i32, name: &str) -> SceneListEntry {
    SceneListEntry {
        index,
        name: name.to_string(),
    }
}

#[test]
fn reconciliation_tracks_scene_move_later() {
    let mut state = ShowState {
        lockout: false,
        scene_configs: vec![
            named_scene_config(0, "Intro", 100),
            named_scene_config(1, "Verse", 200),
            named_scene_config(2, "Chorus", 300),
        ],
    };

    assert!(state.reconcile_scene_fade_configs(&[
        scene_entry(0, "Verse"),
        scene_entry(1, "Chorus"),
        scene_entry(2, "Intro"),
    ]));

    assert_eq!(state.scene_configs[0].scene_id, "0::Verse");
    assert_eq!(state.scene_configs[0].duration_ms, 200);
    assert_eq!(state.scene_configs[1].scene_id, "1::Chorus");
    assert_eq!(state.scene_configs[1].duration_ms, 300);
    assert_eq!(state.scene_configs[2].scene_id, "2::Intro");
    assert_eq!(state.scene_configs[2].duration_ms, 100);
}

#[test]
fn reconciliation_tracks_scene_move_earlier() {
    let mut state = ShowState {
        lockout: false,
        scene_configs: vec![
            named_scene_config(0, "Intro", 100),
            named_scene_config(1, "Verse", 200),
            named_scene_config(2, "Chorus", 300),
        ],
    };

    assert!(state.reconcile_scene_fade_configs(&[
        scene_entry(0, "Chorus"),
        scene_entry(1, "Intro"),
        scene_entry(2, "Verse"),
    ]));

    assert_eq!(state.scene_configs[0].scene_id, "0::Chorus");
    assert_eq!(state.scene_configs[0].duration_ms, 300);
    assert_eq!(state.scene_configs[1].scene_id, "1::Intro");
    assert_eq!(state.scene_configs[1].duration_ms, 100);
    assert_eq!(state.scene_configs[2].scene_id, "2::Verse");
    assert_eq!(state.scene_configs[2].duration_ms, 200);
}

#[test]
fn reconciliation_tracks_single_insert() {
    let mut state = ShowState {
        lockout: false,
        scene_configs: vec![named_scene_config(0, "Intro", 100), named_scene_config(1, "Chorus", 300)],
    };

    assert!(state.reconcile_scene_fade_configs(&[
        scene_entry(0, "Intro"),
        scene_entry(1, "Verse"),
        scene_entry(2, "Chorus"),
    ]));

    assert_eq!(state.scene_configs[0].scene_id, "0::Intro");
    assert_eq!(state.scene_configs[0].duration_ms, 100);
    assert_eq!(state.scene_configs[1].scene_id, "1::Verse");
    assert_eq!(state.scene_configs[1].duration_ms, 0);
    assert!(state.scene_configs[1].channel_configs.is_empty());
    assert_eq!(state.scene_configs[2].scene_id, "2::Chorus");
    assert_eq!(state.scene_configs[2].duration_ms, 300);
}

#[test]
fn reconciliation_tracks_single_delete() {
    let mut state = ShowState {
        lockout: false,
        scene_configs: vec![
            named_scene_config(0, "Intro", 100),
            named_scene_config(1, "Verse", 200),
            named_scene_config(2, "Chorus", 300),
        ],
    };

    assert!(state.reconcile_scene_fade_configs(&[scene_entry(0, "Intro"), scene_entry(1, "Chorus")]));

    assert_eq!(state.scene_configs.len(), 2);
    assert_eq!(state.scene_configs[0].scene_id, "0::Intro");
    assert_eq!(state.scene_configs[0].duration_ms, 100);
    assert_eq!(state.scene_configs[1].scene_id, "1::Chorus");
    assert_eq!(state.scene_configs[1].duration_ms, 300);
}

#[test]
fn reconciliation_uses_exact_match_fallback_for_multi_operation_change() {
    let mut state = ShowState {
        lockout: false,
        scene_configs: vec![named_scene_config(0, "Intro", 100), named_scene_config(1, "Verse", 200)],
    };

    assert!(state.reconcile_scene_fade_configs(&[scene_entry(0, "Intro New"), scene_entry(1, "Verse New")]));

    assert_eq!(state.scene_configs[0].scene_id, "0::Intro New");
    assert_eq!(state.scene_configs[0].duration_ms, 0);
    assert_eq!(state.scene_configs[1].scene_id, "1::Verse New");
    assert_eq!(state.scene_configs[1].duration_ms, 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p advanced-show-control show::state::tests::reconciliation_tracks_scene_move_later show::state::tests::reconciliation_tracks_scene_move_earlier show::state::tests::reconciliation_tracks_single_insert show::state::tests::reconciliation_tracks_single_delete show::state::tests::reconciliation_uses_exact_match_fallback_for_multi_operation_change`

Expected: movement/insert/delete tests fail because only rename is implemented.

- [ ] **Step 3: Implement classifier and transform helpers**

Add this enum and helpers near the private scene-list helpers in `src/show/state.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
enum SceneListChange {
    Noop,
    Rename,
    Move { from: usize, to: usize },
    Insert { at: usize },
    Delete { at: usize },
    Ambiguous,
}

fn without_at(entries: &[SceneEntry], at: usize) -> Vec<SceneEntry> {
    entries
        .iter()
        .enumerate()
        .filter_map(|(idx, entry)| if idx == at { None } else { Some(entry.clone()) })
        .collect()
}

fn classify_scene_list_change(old: &[SceneEntry], new: &[SceneEntry]) -> SceneListChange {
    if old == new {
        return SceneListChange::Noop;
    }

    if old.len() == new.len() {
        let changed_indexes: Vec<_> = old
            .iter()
            .zip(new.iter())
            .enumerate()
            .filter_map(|(idx, (old, new))| if old == new { None } else { Some(idx) })
            .collect();
        if changed_indexes.len() == 1 {
            let idx = changed_indexes[0];
            if old[idx].index == new[idx].index {
                return SceneListChange::Rename;
            }
        }

        let mut matches = Vec::new();
        for from in 0..old.len() {
            let moved = old[from].clone();
            let remaining = without_at(old, from);
            for to in 0..old.len() {
                let mut candidate = remaining.clone();
                candidate.insert(to, moved.clone());
                let candidate_names: Vec<_> = candidate.iter().map(|entry| entry.name.as_str()).collect();
                let new_names: Vec<_> = new.iter().map(|entry| entry.name.as_str()).collect();
                if candidate_names == new_names {
                    matches.push((from, to));
                }
            }
        }
        matches.sort_unstable();
        matches.dedup();
        return match matches.as_slice() {
            [(from, to)] if from != to => SceneListChange::Move { from: *from, to: *to },
            _ => SceneListChange::Ambiguous,
        };
    }

    if new.len() == old.len() + 1 {
        let matches: Vec<_> = (0..new.len())
            .filter(|at| {
                let candidate = without_at(new, *at);
                let candidate_names: Vec<_> = candidate.iter().map(|entry| entry.name.as_str()).collect();
                let old_names: Vec<_> = old.iter().map(|entry| entry.name.as_str()).collect();
                candidate_names == old_names
            })
            .collect();
        return match matches.as_slice() {
            [at] => SceneListChange::Insert { at: *at },
            _ => SceneListChange::Ambiguous,
        };
    }

    if old.len() == new.len() + 1 {
        let matches: Vec<_> = (0..old.len())
            .filter(|at| {
                let candidate = without_at(old, *at);
                let candidate_names: Vec<_> = candidate.iter().map(|entry| entry.name.as_str()).collect();
                let new_names: Vec<_> = new.iter().map(|entry| entry.name.as_str()).collect();
                candidate_names == new_names
            })
            .collect();
        return match matches.as_slice() {
            [at] => SceneListChange::Delete { at: *at },
            _ => SceneListChange::Ambiguous,
        };
    }

    SceneListChange::Ambiguous
}
```

Then update `reconcile_scene_fade_configs` to match on the classifier:

```rust
pub fn reconcile_scene_fade_configs(&mut self, scenes: &[SceneListEntry]) -> bool {
    let old_entries = entries_from_configs(&self.scene_configs);
    let new_entries = entries_from_scene_list(scenes);

    match classify_scene_list_change(&old_entries, &new_entries) {
        SceneListChange::Noop => false,
        SceneListChange::Rename => self.apply_position_mapping(&new_entries),
        SceneListChange::Move { from, to } => {
            let mut next = self.scene_configs.clone();
            let moved = next.remove(from);
            next.insert(to, moved);
            self.replace_scene_configs_with_entries(next, &new_entries)
        }
        SceneListChange::Insert { at } => {
            let mut next = self.scene_configs.clone();
            next.insert(at, default_scene_config(&new_entries[at]));
            self.replace_scene_configs_with_entries(next, &new_entries)
        }
        SceneListChange::Delete { at } => {
            let mut next = self.scene_configs.clone();
            next.remove(at);
            self.replace_scene_configs_with_entries(next, &new_entries)
        }
        SceneListChange::Ambiguous => self.reconcile_scene_fade_configs_by_exact_match(&new_entries),
    }
}
```

Add these private methods inside `impl ShowState`:

```rust
fn apply_position_mapping(&mut self, entries: &[SceneEntry]) -> bool {
    self.replace_scene_configs_with_entries(self.scene_configs.clone(), entries)
}

fn replace_scene_configs_with_entries(
    &mut self,
    mut configs: Vec<SceneConfig>,
    entries: &[SceneEntry],
) -> bool {
    for (config, entry) in configs.iter_mut().zip(entries.iter()) {
        update_scene_locator(config, entry);
    }
    let changed = configs != self.scene_configs;
    self.scene_configs = configs;
    changed
}

fn reconcile_scene_fade_configs_by_exact_match(&mut self, entries: &[SceneEntry]) -> bool {
    let mut next = Vec::with_capacity(entries.len());
    for entry in entries {
        let id = scene_id(entry.index, &entry.name);
        if let Some(existing) = self.scene_configs.iter().find(|scene| scene.scene_id == id) {
            next.push(existing.clone());
        } else {
            next.push(default_scene_config(entry));
        }
    }

    let changed = next != self.scene_configs;
    self.scene_configs = next;
    changed
}
```

- [ ] **Step 4: Run focused reconciliation tests**

Run: `cargo test -p advanced-show-control show::state::tests::reconciliation_`

Expected: all reconciliation tests pass. If the filter does not match all tests on this Cargo version, run `cargo test -p advanced-show-control show::state::tests`.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/show/state.rs
git commit -m "feat: track scene list edits"
```

---

### Task 3: Add Duplicate Scene Warning In React

**Files:**
- Modify: `ui/src/components/SceneTab.tsx`

- [ ] **Step 1: Add duplicate-name helper**

In `ui/src/components/SceneTab.tsx`, add this helper above `SceneTab`:

```tsx
function duplicateSceneNames(scenes: SceneConfig[]): string[] {
  const counts = new Map<string, number>();
  for (const scene of scenes) {
    counts.set(scene.sceneName, (counts.get(scene.sceneName) ?? 0) + 1);
  }

  return [...counts.entries()]
    .filter(([, count]) => count > 1)
    .map(([name]) => name)
    .sort((a, b) => a.localeCompare(b));
}
```

- [ ] **Step 2: Render persistent warning**

Inside `SceneTab`, after `selected` is declared, add:

```tsx
const duplicateNames = duplicateSceneNames(props.appState.sceneConfigs);
```

Then render this block after the scene section description and before the scrollable scene list:

```tsx
{duplicateNames.length > 0 ? (
  <div className="mt-4 rounded-lg border border-amber-500/40 bg-amber-950/40 p-3 text-sm text-amber-100">
    <p className="font-semibold">Scene tracking warning</p>
    <p className="mt-1 text-amber-100/80">
      Duplicate scene names make some LV1 scene moves hard to track. Rename duplicate scenes for the most reliable scene tracking.
    </p>
    <p className="mt-1 text-xs text-amber-100/70">Duplicates: {duplicateNames.join(", ")}</p>
  </div>
) : null}
```

- [ ] **Step 3: Run frontend typecheck**

Run: `npm run typecheck`

Expected: PASS.

- [ ] **Step 4: Commit**

Run:

```bash
git add ui/src/components/SceneTab.tsx
git commit -m "feat: warn about duplicate scene names"
```

---

### Task 4: Documentation And Verification

**Files:**
- Modify: `docs/scene-tracking.md` only if implementation behavior differs from the current doc.

- [ ] **Step 1: Check docs against implementation**

Read `docs/scene-tracking.md` and confirm it still matches implemented behavior:

```text
Scene tracking supports deterministic rename, move, insert, and delete.
Ambiguous transitions fall back to exact matching.
React warns on duplicate scene names.
Recall validation remains exact index/name.
```

- [ ] **Step 2: Run Rust formatting check**

Run: `cargo fmt --all -- --check`

Expected: PASS.

- [ ] **Step 3: Run focused Rust tests**

Run: `cargo test -p advanced-show-control show::state::tests`

Expected: PASS.

- [ ] **Step 4: Run broader relevant Rust tests**

Run: `cargo test -p advanced-show-control-tauri scene_recall`

Expected: PASS. This confirms exact scene recall policy still works.

- [ ] **Step 5: Run frontend build**

Run: `npm run build`

Expected: PASS.

- [ ] **Step 6: Commit docs or verification-only final state**

If docs changed, run:

```bash
git add docs/scene-tracking.md
git commit -m "docs: update scene tracking behavior"
```

If docs did not change, do not create an empty commit.

---

## Self-Review Notes

- Spec coverage: Rust tasks cover `ShowState`-owned deterministic rename/move/insert/delete and exact fallback. React task covers persistent duplicate-name warning. Verification task covers safety and docs.
- Placeholder scan: no `TBD`, no generic edge-case steps, no unspecified tests.
- Type consistency: plan uses existing `SceneConfig`, `SceneListEntry`, `scene_id`, and `ShowState::reconcile_scene_fade_configs` names.
