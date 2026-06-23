# Desk Research Findings: Session And LV1 Scene Alignment

## Method And Confidence

This is a secondary research synthesis, not participant research. It uses public Waves documentation, Waves forum discussions, and adjacent live-console automation guidance to infer likely expectations from LV1 and live-sound workflows. Confidence is strongest where findings are supported by LV1-specific sources. Confidence is moderate where extrapolated from Yamaha, Midas, DiGiCo, touring, theatre, and church-production workflows.

Key evidence: LV1 scenes are snapshots stored inside sessions, with a scene list, scene notes, recall, store, scope, and recall-safe behavior. Waves also supports offline LV1 session preparation, which validates the “program before arriving or before connecting” workflow.

## Top User Workflows

### 1. Offline Or Template-First Preparation

Engineers are likely to prepare a show file before arriving at the venue or before the live LV1 system is available. Waves explicitly markets the LV1 Session Editor for offline preparation and loading the finished session into the main LV1 system later.

Implication: Users will expect the app to support a disconnected state, but they should not be allowed to assume fade automation is ready until the current LV1 scene list has been checked.

### 2. Scene-Per-Song Or Scene-Per-Section Operation

Public live-sound guidance describes one-scene-per-song workflows, and LV1-specific guidance describes scenes for songs or song sections using scoped parameters such as pan, mute, and fader.

Implication: Scene order matters because it often mirrors the set list, service flow, or cue order. Scene name also matters, but name alone is not enough.

### 3. Theatre Or Highly Automated Cue Workflows

Theatre-style workflows may use many scenes, sometimes every few lines, to handle performers entering and leaving stage.

Implication: In high-density cue lists, accidental remapping is especially dangerous. A small mismatch can affect many following cues.

### 4. Touring And Venue-Reuse Workflows

Touring engineers often carry show files between consoles or venues for consistency and speed, but public guidance emphasizes that loading show files can be risky if outputs, routing, or venue-specific data are not protected.

Implication: “Wrong LV1 show or console” should be treated as a real safety scenario, not an edge case.

### 5. External Show-Control Integration

A Waves forum user describes LV1 scenes being recalled with QLab and MIDI program changes, where reordering LV1 scenes causes the wrong cues to fire because scene numbers change with list order. Another user confirms that changing the LV1 scene list can make “everything go wrong.”

Implication: Matching by scene index alone is unsafe. Reordering is not cosmetic; it can break automation.

## Common Scene-List Change Patterns

| Change Pattern | Likely Frequency | Risk Level | Research Interpretation |
| -------------- | ---------------- | ---------- | ----------------------- |
| Scene renamed | Common | Medium | Names evolve during rehearsal, especially from rough labels to final song or section names. Safe only when other evidence matches. |
| Scene moved/reordered | Common | High | Set lists and show orders change. Forum evidence shows LV1 reordering can break external automation tied to scene numbers. |
| Scene inserted | Common | Medium to High | Normal during rehearsal; shifts indexes and may change surrounding context. |
| Scene deleted | Common | High | Saved fade metadata may now point to nothing. Preserve rather than discard. |
| Scene duplicated | Plausible | High | Duplicate names like “Verse,” “Blackout,” or “Walkout” are realistic in show workflows. Name-only matching becomes unsafe. |
| Session rebuilt from template | Common | High | Public guidance warns that inherited or template show files can contain hidden assumptions. |
| Wrong show or console | Less frequent, severe | Critical | Loading a show file can cause unsafe recall behavior if venue-specific parameters are not protected. |

## How Engineers Likely Think About Scene Identity

The safest model is composite identity, not one field.

Engineers likely use:

- Scene name: useful and human-readable, but unstable and non-unique.
- Scene order/index: operationally important, but unsafe as identity because LV1 renumbers scenes when reordered.
- Neighboring scenes: “This Verse sits between Intro and Chorus” may be more trustworthy than name alone.
- Notes: LV1 scene notes are visible for the selected scene, so they are likely meaningful context.
- Scoped parameters: LV1 workflows rely on scope/filter choices to decide what changes on recall.
- Fade metadata weight: stored fader targets, scoped fader count, duration, and automation-enabled state help users decide whether a saved fade belongs to a current scene.

## Recommended Alignment States And Wording

| State | User-Facing Meaning | Automation Behavior |
| ----- | ------------------- | ------------------- |
| Aligned | The saved app session matches the current LV1 scene list. | Fades can run. |
| Aligned With Changes | Clear changes were handled automatically; review summary is available. | Fades can run for affected scenes, with a visible summary. |
| Review Required | Some saved fade settings may match current LV1 scenes, but confirmation is needed. | Block unresolved scenes; allow confirmed scenes only if the UI makes this obvious. |
| Not Aligned | The app cannot safely associate this session with the current LV1 show. | Global automation lockout until the user resolves or starts fresh. |

Avoid “Unaligned” as the main user-facing word. “Not Aligned” is clearer and less technical. Use “Review Required” when automation is blocked.

Best terminology: Align, Review, Match, Missing, New, Duplicate Name, Moved, Renamed, Not Aligned.

Use “Remap” for manual reassignment. Avoid relying on “reconcile” as the primary label; it is accurate but sounds more like data-management language than live-show language.

## Automatic Versus Review-Required Changes

### Safe To Handle Automatically

Automatic alignment is acceptable only when confidence is high and the action is reversible.

| Situation | Recommendation |
| --------- | -------------- |
| Exact name match, same relative order, unique scene | Auto-match. |
| Exact name match, moved, unique name, same neighboring context | Auto-match and show in summary. |
| New LV1 scene with no saved fade metadata | Add with default fade settings. |
| Saved scene has no custom fade data and no automation enabled | Lower-risk; can auto-archive or mark inactive, but still show summary. |

### Should Require Review

| Situation | Reason |
| --------- | ------ |
| Name changed but index/order is similar | Could be a real rename or a different cue inserted into the same slot. |
| Scene moved a long distance | Set-list reordering is normal, but movement can break automation assumptions. |
| Insertions/deletions near saved fade scenes | Neighbor context changed, so confidence drops. |
| Duplicate names | Name-only matching is unsafe. |
| Saved scene has stored fader targets or automation enabled | Higher consequence if wrong. |
| Low overall match rate | Possible wrong LV1 show or wrong console/session. |
| Scene count differs dramatically | Possible rebuild from template or wrong show. |
| Multiple possible matches | User must choose. |

### Should Block With Strong Warning

| Situation | Recommendation |
| --------- | -------------- |
| Very few saved scenes match current LV1 scenes | Treat as Not Aligned. Block automation globally. |
| Duplicate names plus stored targets | Require explicit mapping before those fades can run. |
| Missing saved scenes with automation enabled | Preserve fade config, but block it from running. |
| Wrong show suspected | Require explicit confirmation before any automation is enabled. |

## Required Review UI Information

A useful alignment review screen should show more than name-to-name matching.

Recommended columns:

| Saved App Session | Current LV1 Scene | State | Evidence | Fade Impact | Action |
| ----------------- | ----------------- | ----- | -------- | ----------- | ------ |
| Scene number and name at save time | Current scene number and name | Matched, Renamed, Moved, Missing, New, Duplicate, Review Required | Name match, order match, neighboring scenes, notes | Duration, scoped fader count, stored target count, automation enabled | Accept, Change Match, Keep Unresolved, Disable Fade, Use Defaults |

Required details:

- Saved scene number and current LV1 scene number.
- Saved name and current name.
- Previous and current neighboring scenes.
- Scene notes, where available.
- Fade duration.
- Count of scoped faders.
- Count of stored target values.
- Whether automation is enabled.
- Last saved time and session file name.
- Confidence indicator in plain language, such as Strong Match, Possible Match, or Ambiguous.
- Clear reason text, such as “Same name, moved from Scene 8 to Scene 10.”

The review screen should support bulk acceptance only for high-confidence changes. Ambiguous rows should require individual confirmation.

## Duplicate Scene Names

Duplicate names should be treated as a first-class state, not an error.

Likely duplicate-name examples:

- Verse
- Chorus
- Blackout
- Walkout
- Pastor
- Band
- Playback
- MC

For duplicate names, the app should show:

- All candidate LV1 scenes with scene number.
- Surrounding scenes before and after each candidate.
- Notes.
- Fade metadata preview.
- Whether the saved fade had targets or automation enabled.
- A manual selection control.

Automatic matching by duplicate name alone should not be allowed.

## Missing Saved Scenes

Saved fade settings for missing scenes should be preserved, not discarded.

Recommended treatment:

- Move them into a Saved But Not In LV1 section.
- Show whether each missing scene contains meaningful fade work.
- Allow users to remap to an LV1 scene.
- Allow users to keep unresolved for later.
- Allow users to disable automation.
- Allow users to delete manually.

Suggested warning language:

“This saved fade setup is not linked to any current LV1 scene. It will not run unless you link it to a scene.”

For scenes with stored targets or automation enabled:

“This scene has saved fader targets, so it needs review before automation can run.”

Avoid panic language such as “lost,” “corrupt,” or “failed” unless data is actually unavailable.

## Automation Blocking Expectations

The safest default is:

- Aligned: allow automation.
- Aligned With Changes: allow automation, but show a persistent summary until dismissed.
- Review Required: allow automation only for confirmed aligned scenes; block unresolved scenes visibly.
- Not Aligned: global automation lockout.

Per-scene blocking is useful but must be unmistakable during a show. The app should show blocked scenes directly in the scene list with a lock or warning label, and recall/fade controls should explain why the fade is unavailable.

Recommended blocked-state copy:

“Fade blocked: Scene match needs review.”

For global lockout:

“Fade automation is off because this app session is not aligned with the current LV1 show.”

This aligns with broader live-console safety norms: public guidance repeatedly emphasizes recall-safe behavior, output protection, and avoiding unexpected parameter changes during scene recall.

## Scenario-Specific Findings

### Scenario A: Template Session Before LV1 Connection

Expected behavior:

- App should allow editing fade metadata while disconnected.
- App should label the session as Not Yet Aligned or Waiting For LV1 Scene List.
- On connection, app should run alignment automatically.
- Automation should remain unavailable until alignment finishes.
- If alignment is clean, show Aligned or Aligned With Changes.
- If ambiguous, show Review Required.

### Scenario B: LV1 Programmed First, App Session Loaded Later

Expected behavior:

- App should compare the loaded session to the current LV1 scene list immediately.
- Exact unique matches can attach automatically.
- Name mismatches, missing scenes, duplicate names, or low match rate should require review.
- The user needs scene number, name, order, notes, and fade metadata to decide.

### Scenario C: Rehearsal Changes Between Saves

Expected behavior:

- Renames, inserts, moves, and deletes are normal.
- Engineers likely expect fade programming to follow the “same cue,” not blindly follow index.
- Scene number is useful as evidence but unsafe as identity.
- Rehearsal changes should produce a summary, not silent remapping.

### Scenario D: Duplicate Scene Names

Expected behavior:

- Duplicate names should require explicit user selection.
- The app should not auto-match duplicates unless there is another strong identifier.
- Show surrounding scene context and fade details.

### Scenario E: Missing Saved Scenes

Expected behavior:

- Preserve fade settings.
- Do not run automation for missing scenes.
- Offer remap or keep unresolved.
- Prioritize review if stored targets exist.

### Scenario F: Wrong LV1 Show Or Console

Expected behavior:

- Detect using low match rate, large scene-count difference, different session/show metadata if available, and mismatched scene order.
- Block automation globally.
- Present a clear “This may be the wrong LV1 show” warning.
- Require explicit confirmation to continue.

## Concrete User Stories

- As an engineer using a template before LV1 is connected, I want the app to say the session is not yet aligned so I do not assume fades are ready.
- As an engineer loading an old app session against a changed LV1 show, I want to review missing or ambiguous scenes before automation can run.
- As an engineer whose set list changed, I want fade settings to follow the intended scene when the match is clear, and I want a summary of what moved.
- As an engineer with duplicate scene names, I want to choose exactly which LV1 scene receives each saved fade configuration.
- As a system tech preparing a session for another operator, I want unresolved mappings to be blocked clearly so the operator cannot accidentally trigger unsafe fades.

## Open Questions For Future Primary Research

- Do LV1 engineers commonly reuse exact duplicate scene names, or do they usually add numbers such as “Verse 1” and “Verse 2”?
- How much do LV1 scene notes get used in real workflows?
- Would engineers prefer per-scene blocking or global lockout during a live show?
- What confidence threshold feels acceptable for automatic renamed matches?
- Should the app maintain its own persistent scene identifier once a mapping is confirmed?
- Should users be able to export or print an alignment report for show documentation?

## Product Recommendation Summary

Build alignment around a conservative rule: preserve useful fade programming, but never silently attach automation when scene identity is ambiguous.

The app should:

- Treat LV1 as the source of truth for scene list, order, and names.
- Store app-side fade metadata with enough context to rematch later.
- Auto-align only exact or high-confidence unique matches.
- Require review for renamed, duplicate, missing, or low-confidence matches.
- Preserve unmatched fade settings.
- Block unresolved automation.
- Use plain-language states: Aligned, Aligned With Changes, Review Required, Not Aligned.
