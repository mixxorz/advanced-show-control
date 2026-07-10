# Cue List Scene ID Preservation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve cue-list scene UUID references when reopening a session whose referenced scenes have no fade metadata.

**Architecture:** Treat every persisted scene config as a durable scene identity record by importing blank/default configs instead of pruning them. Keep the existing scene alignment, cue-list reconciliation, recall routing, and safety validation unchanged.

**Tech Stack:** Rust, Tokio actors, serde, UUID, cargo-nextest

## Global Constraints

- Do not add backward-compatibility recovery for sessions whose cue references have no corresponding scene config.
- Do not change the show-file schema, cue-list data model, frontend contract, scene alignment heuristics, or recall routing.
- Ambiguous scene matches must remain unresolved rather than guessing.
- Cue recall must continue through `ScenesCommand::RecallScene` and existing LV1 identity validation.
- Rust behavior coverage must use pure unit tests for import and an actor test through actor mailboxes for session load.

---

## File Structure

- Modify `src-tauri/src/show/show_file.rs`: retain every stored scene config during import and update pure import coverage.
- Modify `src-tauri/src/show/actor.rs`: replace the obsolete pruning test with an actor-level cue-list session-load regression.

No new production files, types, commands, or interfaces are required.

### Task 1: Preserve Persisted Scene Identities During Session Load

**Files:**
- Modify: `src-tauri/src/show/show_file.rs:123-175,252-469`
- Modify: `src-tauri/src/show/actor.rs:565-903`
- Test: `src-tauri/src/show/show_file.rs`
- Test: `src-tauri/src/show/actor.rs`

**Interfaces:**
- Consumes: `import_show_file(file: &mut ShowFile, lv1: &Lv1StateSnapshot) -> Result<ImportedShowFile, String>` and the existing `load_show_file_from_dto` actor workflow.
- Produces: unchanged interfaces; `ImportedShowFile::snapshot.scene_configs` now includes blank/default persisted configs.

- [ ] **Step 1: Add a pure import regression test**

In `src-tauri/src/show/show_file.rs`, add this test to the existing `tests` module:

```rust
#[test]
fn import_show_file_preserves_blank_scene_config_identity() {
    let scene_id = uuid::Uuid::from_u128(0x11111111111141118111111111111111);
    let mut file = ShowFile {
        schema_version: SHOW_FILE_SCHEMA_VERSION,
        app_version: "test".to_string(),
        saved_at: "123".to_string(),
        safety: ShowFileSafety { lockout: false },
        scene_configs: vec![ShowFileSceneConfig {
            internal_scene_id: Some(scene_id),
            scene_index: Some(1),
            scene_name: "Intro".to_string(),
            duration_ms: 0,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: ShowFileSceneScopeToggles::default(),
        }],
        cue_lists: Vec::new(),
        active_cue_list_id: None,
        cued_cue_entry_id: None,
    };
    let lv1 = Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: vec![SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }],
        channels: Vec::new(),
    };

    let imported = import_show_file(&mut file, &lv1).unwrap();

    assert_eq!(imported.snapshot.scene_configs.len(), 1);
    assert_eq!(
        imported.snapshot.scene_configs[0].internal_scene_id,
        scene_id
    );
    assert_eq!(imported.snapshot.scene_configs[0].scene_index, Some(1));
    assert_eq!(imported.snapshot.scene_configs[0].scene_name, "Intro");
}
```

- [ ] **Step 2: Replace the obsolete actor pruning test with the cue-list load regression**

In `src-tauri/src/show/actor.rs`, replace `connected_load_drops_blank_file_configs_before_alignment` with:

```rust
#[tokio::test]
async fn connected_load_preserves_default_scene_ids_referenced_by_cue_entries() {
    let event_bus = AppEventBus::default();
    let (show, peers) = show_actor(event_bus.clone());
    let (scenes, task, _scenes_peers) =
        build_scenes_actor(1, RuntimeGeneration::default(), event_bus.clone());
    task.spawn();
    peers.set_scenes(scenes.clone());
    let (cue_lists, task, _cue_lists_peers) =
        build_cue_lists_actor_with_scenes(event_bus, scenes.clone());
    task.spawn();
    peers.set_cue_lists(cue_lists.clone());

    let lv1 = lv1_snapshot(vec![
        SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        },
        SceneListEntry {
            index: 2,
            name: "Verse".to_string(),
        },
    ]);
    let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(4);
    let lv1_handle = crate::lv1::test_actor_handle(lv1_tx);
    tokio::spawn(async move {
        while let Some(command) = lv1_rx.recv().await {
            if let crate::lv1::Lv1Command::GetState { reply } = command {
                let _ = reply.send(lv1.clone());
            }
        }
    });
    peers.set_lv1(1, lv1_handle);

    let path = std::env::temp_dir().join(format!("show-load-cue-ids-{}.ascs", Uuid::new_v4()));
    let intro_id = Uuid::from_u128(1);
    let verse_id = Uuid::from_u128(2);
    let cue_list_id = Uuid::from_u128(3);
    let intro_entry_id = Uuid::from_u128(4);
    let verse_entry_id = Uuid::from_u128(5);
    let mut file = show_file(vec![
        file_scene(scene_config(1, Some(1), "Intro", 0)),
        file_scene(scene_config(2, Some(2), "Verse", 0)),
    ]);
    file.cue_lists = vec![crate::cue_lists::CueList {
        id: cue_list_id,
        name: "Main".to_string(),
        entries: vec![
            crate::cue_lists::CueEntry {
                id: intro_entry_id,
                scene_internal_id: intro_id,
            },
            crate::cue_lists::CueEntry {
                id: verse_entry_id,
                scene_internal_id: verse_id,
            },
        ],
    }];
    file.active_cue_list_id = Some(cue_list_id);
    file.cued_cue_entry_id = Some(intro_entry_id);

    crate::show_file::write_show_file(&path, &file, &crate::show_file::backup_folder())
        .unwrap();

    let (reply, rx) = tokio::sync::oneshot::channel();
    show.send(ShowCommand::LoadShowFileFromPath {
        path: path.clone(),
        reply: Some(reply),
    })
    .await
    .unwrap();
    assert!(rx.await.unwrap().is_ok());

    let scene_document = get_scene_document(&scenes).await;
    assert_eq!(scene_document.scene_configs.len(), 2);
    assert_eq!(scene_document.scene_configs[0].internal_scene_id, intro_id);
    assert_eq!(scene_document.scene_configs[1].internal_scene_id, verse_id);

    let cue_document = get_cue_list_document(&cue_lists).await;
    assert_eq!(cue_document.cue_lists[0].entries.len(), 2);
    assert_eq!(
        cue_document.cue_lists[0].entries[0].scene_internal_id,
        intro_id
    );
    assert_eq!(
        cue_document.cue_lists[0].entries[1].scene_internal_id,
        verse_id
    );
    assert_eq!(cue_document.cued_cue_entry_id, Some(intro_entry_id));

    std::fs::remove_file(path).unwrap();
}
```

- [ ] **Step 3: Run both regressions and verify they fail for the reported cause**

Run:

```bash
cargo nextest run -p advanced-show-control -E 'test(import_show_file_preserves_blank_scene_config_identity) or test(connected_load_preserves_default_scene_ids_referenced_by_cue_entries)'
```

Expected: both tests fail because `import_show_file` removes blank scene configs. The pure test reports zero imported configs; the actor test reports regenerated UUIDs or a cleared cued entry.

- [ ] **Step 4: Remove blank scene-config pruning**

In `src-tauri/src/show/show_file.rs`, delete this statement from `import_show_file`:

```rust
file.scene_configs
    .retain(|config| !is_blank_scene_config(config));
```

Delete the now-unused helper:

```rust
fn is_blank_scene_config(config: &ShowFileSceneConfig) -> bool {
    config.duration_ms == 0
        && config.channel_configs.is_empty()
        && config.scoped_channels.is_empty()
        && config.scope_toggles == ShowFileSceneScopeToggles::default()
}
```

Do not change `generated_internal_scene_ids`, `file_scene_to_show_scene`, `align_scene_configs`, cue-list reconciliation, or recall code.

- [ ] **Step 5: Run the focused regressions and show module tests**

Run:

```bash
cargo nextest run -p advanced-show-control -E 'test(import_show_file_preserves_blank_scene_config_identity) or test(connected_load_preserves_default_scene_ids_referenced_by_cue_entries) or test(/show::/)'
```

Expected: all selected tests pass, including the existing legacy missing-ID test and actor load tests.

- [ ] **Step 6: Run Rust formatting, linting, and the full Rust test suite**

Run:

```bash
make rust-fmt
make rust-lint
make rust-test
```

Expected: formatting check, clippy with warnings denied, and all Rust tests pass.

- [ ] **Step 7: Inspect and commit the implementation**

Run:

```bash
git status --short
git diff --check
git diff -- src-tauri/src/show/show_file.rs src-tauri/src/show/actor.rs
git log --oneline -10
git add src-tauri/src/show/show_file.rs src-tauri/src/show/actor.rs
git commit -m "fix: preserve cue list scene IDs on load"
```

Expected: only the two intended Rust files are staged, hooks pass, and the commit is created without including unrelated changes.
