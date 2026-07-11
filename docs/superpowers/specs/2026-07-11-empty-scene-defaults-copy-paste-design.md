# Empty Scene Defaults and Scene Settings Copy/Paste Design

## Purpose

This design combines GitHub issues #46 and #45. It makes new scene fade configurations fail safe by default and implements the existing Copy and Paste actions for scene fade settings.

The work remains limited to app-owned scene configuration. It must not recall an LV1 scene, start or abort a fade, send parameter writes, or change active-fade behavior.

## Empty Scene Defaults

`SceneScopeToggles::default()` will disable both fader and pan scope. A newly created scene configuration will contain:

- `faders: false`
- `pan: false`
- No channel configurations
- No scoped channels

Scene creation, new-session reconciliation, and newly discovered scene alignment will all use this empty default. The UI will consequently show no parameters or channels selected until the engineer explicitly opts them into app-managed behavior.

Show files that explicitly contain scope values will continue to load those values unchanged. Older show files whose scope fields are omitted will adopt the new fail-safe default and load with both scope toggles disabled. This is an intentional behavior change: omitted fields do not preserve the previous implicit fader scope.

Recall policy will treat an empty configuration as unmanaged. Recalling such a scene must not start a fade, abort an existing fade, or send fader or pan commands.

## Clipboard Ownership

The Scenes actor will own an ephemeral, app-local scene-settings clipboard. The clipboard will contain an owned snapshot of only the editable fade settings:

- Fade duration
- Scope toggles
- Channel configurations and stored targets
- Scoped-channel references

The clipboard will never contain the source scene's internal ID, LV1 scene index, or scene name. Cloning the settings into actor-owned state ensures later edits to the source cannot change the copied snapshot.

The clipboard will be cleared when the scene document is replaced for a newly created or opened session. It will not persist across app restarts. Clearing it on session replacement prevents settings captured from one LV1/show context from being applied to another.

## Commands and Data Flow

The Scenes actor mailbox will expose explicit Copy and Paste commands.

Copy will:

1. Validate that the source scene configuration exists.
2. Clone its editable settings into the clipboard.
3. Publish clipboard availability when it changes.
4. Leave scene state and session dirty state unchanged.

Paste will:

1. Validate that clipboard settings exist.
2. Validate that the destination scene exists and is linked to an LV1 scene.
3. Build the complete prospective destination configuration while preserving its internal ID, LV1 scene index, and scene name.
4. Compare the prospective configuration with the current destination before mutation.
5. Atomically replace only the editable settings when they differ.
6. Publish one persisted scene-state edit so the session becomes dirty and the projector updates the UI.

Pasting identical settings will return `changed: false`, produce no persisted edit, and avoid misleading state changes or operational logs. Any validation failure will return a frontend-safe error before mutation, leaving the scene document unchanged.

Clipboard availability will be part of the scene projection contract. The frontend will use projected state rather than maintaining a second clipboard or inferring backend state.

## UI Behavior

Copy will be enabled whenever a selected scene configuration exists. A source scene does not need to be linked because an unlinked scene can still contain valid app-managed settings.

Paste will be enabled only when:

- Valid clipboard settings exist.
- A destination scene is selected.
- The destination scene is linked.

Copying an unlinked source copies only its settings. Pasting onto an unlinked destination is disabled in the UI and rejected by the backend if requested directly. If that destination is subsequently linked, Paste becomes available without requiring another Copy.

The existing Copy and Paste controls will dispatch the new commands and expose their disabled states accessibly. Command failures will use the existing frontend command-error path.

## Safety

Copy and Paste update app-owned configuration only. They will not call LV1 or Fade actor commands and will not interact with recall policy while the operation is running.

Paste validation and mutation will occur within the Scenes actor so the update is atomic. The operation will preserve lockout, exact scene identity validation, fresh-state requirements, generation guards, manual override, active-fade ownership, and disconnect safety.

The empty default ensures no channel or parameter becomes app-managed without explicit engineer intent. An empty/default scene cannot cause parameter writes during later recall automation.

## Public Site Documentation

The implementation will update the operator-facing Zensical site with the shipped behavior.

`site/docs/getting-started.md` will revise the first-fade procedure so it no longer assumes **FADER** begins enabled. The procedure will require the engineer to select the intended channels and explicitly enable **FADER** and, when needed, **PAN** before recall.

`site/docs/scenes.md` will:

- State that new scene fade configurations begin with no channels or parameters in scope.
- Explain that **FADER** and **PAN** must be enabled explicitly.
- Replace the obsolete unavailable-controls text with instructions for **Copy** and **Paste**.
- State exactly which settings are copied and that destination scene identity is preserved.
- Explain that Copy is available for unlinked scenes, while Paste requires a linked destination.
- Explain that the clipboard is cleared when the engineer creates or opens another session.
- Explain that pasting identical settings makes no session change.

Scene screenshots sourced from visual regression assets will be refreshed when their visible default scope or Copy/Paste availability no longer matches the implemented UI. Documentation images will continue to be copied into the site as immutable assets rather than linked to generated test output.

## Testing

### Pure Unit Tests

- Verify `SceneScopeToggles::default()` disables fader and pan scope.
- Verify new `SceneConfig` values contain no channel configurations or scoped channels.
- Verify omitted show-file scope fields import as disabled while explicit values remain unchanged.
- Verify setting replacement preserves destination identity fields.
- Verify identical settings are detected as a no-op.
- Verify an empty configuration is skipped by recall policy and produces no fade plan or parameter writes.

### Actor Tests

- Verify new-session and scene-alignment paths use empty defaults.
- Verify Copy snapshots all editable settings from linked and unlinked sources.
- Verify later source edits do not alter the copied snapshot.
- Verify Paste updates a linked destination atomically and leaves the source unchanged.
- Verify changed Paste publishes persisted-edit semantics for dirty tracking and projection.
- Verify identical Paste does not publish a persisted edit.
- Verify missing clipboard, missing destination, and unlinked destination failures do not mutate scene state.
- Verify New/Open scene-document replacement clears the clipboard and projects Paste as unavailable.

Actor tests will interact through the actor mailbox and `AppEventBus`; they will not mutate actor internals or inspect private state directly.

### Frontend and Storybook Tests

- Verify Copy is enabled for linked and unlinked selected scenes.
- Verify Paste is disabled without clipboard data and for unlinked destinations.
- Verify Paste becomes enabled for a linked destination after a successful Copy.
- Verify the controls dispatch the correct commands.
- Add Storybook states showing unavailable and available Paste behavior.

### Debug Smoke Test

The debug smoke suite will exercise production commands and assert projected configuration only:

1. Create a new session and verify a new/default scene projects disabled scope with empty channel settings.
2. Configure source scene A through production commands.
3. Copy A, select linked destination B, and paste through production commands.
4. Verify projected B retains its internal ID, LV1 scene index, and scene name.
5. Verify projected B receives A's duration, scope toggles, channel configurations, and scoped channels.
6. Verify projected A remains unchanged and the session is dirty.
7. Replace the session and verify Paste becomes unavailable.

The added smoke coverage will not recall B or assert live fader movement. Existing smoke coverage remains responsible for live fade execution.

### Documentation Verification

- Verify the site describes the empty default and Copy/Paste behavior without retaining contradictory legacy instructions.
- Run a clean Zensical build from `site/` after updating the manual and any screenshots.
- Check internal links and image references through the site build.

## Implementation Sequence

The changes will be delivered in three reviewable commits:

1. Change new and omitted scene scope defaults to the empty fail-safe model, including persistence, recall-policy, UI, and smoke expectations.
2. Add the backend-owned clipboard, Copy/Paste commands, projection state, UI controls, tests, and smoke workflow.
3. Update the public Zensical manual and affected screenshots for the new defaults and Copy/Paste workflow.

This ordering ensures Copy/Paste is built against the final default and import semantics rather than carrying transitional behavior.
