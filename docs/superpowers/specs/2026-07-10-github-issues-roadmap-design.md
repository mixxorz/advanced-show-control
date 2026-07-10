# GitHub Issues Roadmap Design

## Purpose

GitHub Milestones and Issues will become the source of truth for the product roadmap and actionable work. The repository should stop maintaining `docs/roadmap.md` as a parallel roadmap because parallel trackers will drift.

Milestones describe release intent, scope, safety notes, and exit criteria. Issues describe actionable work. Epic issues group large feature areas into smaller implementation-sized issues.

## Scope

In scope:

- Delete `docs/roadmap.md` during implementation.
- Update `AGENTS.md` so future agents know to inspect GitHub Milestones and Issues for roadmap context.
- Update any repo docs that tell contributors to add future work to `docs/roadmap.md`.
- Add GitHub issue templates for epics, feature/tasks, and bugs.
- Add a project-local opencode skill for opening GitHub issues in this repository.
- Add lightweight label guidance for issue classification.
- Create or prepare the initial GitHub roadmap structure from the current roadmap content.
- Fold Cue Lists into the MVP milestone.
- Remove External Control and Stream Deck from current roadmap tracking.

Out of scope:

- Adding GitHub Projects automation.
- Adding bots or issue synchronization scripts.
- Implementing any product feature listed by the migrated issues.
- Recreating a roadmap mirror in another repository document.
- Creating the project-local opencode skill before the implementation phase.

## Source Of Truth

GitHub Milestones are the release roadmap. Each milestone description should contain:

- Product or release intent.
- In-scope feature areas.
- Explicit out-of-scope notes when useful.
- Safety constraints that apply to the release.
- Exit criteria.

GitHub Issues are the actionable tracker. Each issue should be small enough to close through a focused implementation slice unless it is explicitly labeled as an epic.

Epic issues group large areas inside a milestone. They should contain a task list linking to child issues rather than duplicating all implementation details inline.

Design docs and implementation plans may link to GitHub issues, but they should not become a competing long-term checklist after the issue exists.

## Milestones

Create or maintain these roadmap milestones:

- `MVP: Scene Fades + Cue Lists`
- `Release 2: Event Automation Engine`

Do not create or track External Control or Stream Deck roadmap scope for now. If that direction returns later, it should start as a new milestone or issue after the user explicitly restores it to scope.

## MVP Roadmap Content

The `MVP: Scene Fades + Cue Lists` milestone should cover the existing scene-fade MVP plus Cue Lists.

Milestone description topics:

- Advanced Show Control is a Tauri/Rust/React desktop app for Waves eMotion LV1 and LV1 Classic scene workflows.
- LV1 remains the source of truth for scene creation, scene recall, routing, plugins, mutes, processing, and live mixer state.
- The app is a fader-fade overlay that stores fade metadata for LV1 scenes and moves only scoped faders configured by the engineer.
- The app owns fade metadata, scoped channel targets, fade duration, fade execution, safety behavior, show-file storage, and cue-list workflow.
- Cue Lists are part of MVP and let engineers build show-order lists from LV1 scenes without changing the LV1 scene library order.
- MVP is live-viable only when scene fades, session handling, safety visibility, logging, cue lists, frontend structure, and bundling are trustworthy enough for rehearsal use.

MVP safety notes:

- Do not send fader commands when LV1 is disconnected, stale, unavailable, or unsafe.
- Do not bypass lockout checks, exact scene identity validation, or generation guards.
- Scene recall and cue recall automation must validate before changing active fade ownership.
- Blocked, skipped, disabled, or unsafe recalls must not abort an existing fade.
- Recall fades must start from current live values.
- Manual override, Abort All, overlap/same-scene behavior, and disconnect safety must remain visible and test-covered.

MVP initial issues:

- Epic: MVP completion tracker.
- Evaluate dead-code policy.
- Add session scene reconciliation/remapping.
- Epic: Wire Settings behavior.
- Wire auto-save setting behavior.
- Wire keyboard shortcut behavior.
- Wire auto-cue, time display, and fader override sensitivity follow-ups as focused issues.
- Add auto-session recall.
- Make scene state app-lifetime instead of connection-lifetime.
- Sort out MVP bundling and packaging.
- Epic: Cue Lists.
- Add cue list data model and persistence.
- Add cue list backend actor, commands, and events.
- Add cue list UI create, rename, delete, and reorder behavior.
- Support dragging scenes into cue lists.
- Support cue entry cueing and global Go recall.
- Auto-next after successful cue recall.
- Add cue list status and blocked-recall visibility.
- Add MVP user documentation.

MVP exit criteria:

- A live engineer can connect to LV1, open or create a session, store scoped fader targets for scenes, recall LV1 scenes, observe app-managed fades, abort safely, and understand app state without using a debug console.
- Engineers can manage app session files through the native File menu using `.ascs` files.
- Engineers can create, edit, save, and load cue lists as part of an app session.
- Engineers can recall the selected cue through the app while preserving scene recall and fade safety behavior.
- Cue list recall cannot bypass lockout, scene identity validation, stale-state checks, generation guards, or fade safety rules.
- Logging is split appropriately between diagnostic files and frontend-facing operational events.
- Show-file scene mismatches can be reconciled or remapped without silently dropping app-managed fade configuration.
- Scene state projection is owned by an app-lifetime actor and does not require a connected LV1 runtime to initialize the UI.
- Bundling is good enough for MVP rehearsal/testing distribution.

## Release 2 Roadmap Content

The `Release 2: Event Automation Engine` milestone covers event automation only.

Milestone description topics:

- Engineers can create events with trigger conditions and actions.
- When trigger conditions are met, the app fires configured actions through existing safe command paths.
- Automation decisions must be visible enough to troubleshoot fired, skipped, blocked, or failed actions.

Release 2 safety notes:

- Event automation cannot bypass lockout, scene identity validation, stale-state checks, generation guards, or fade safety rules.
- Automation must prevent unsafe loops or repeated firing.
- Automation-triggered actions must be visible through logs or projected UI state.

Release 2 initial issues:

- Epic: Event Automation release tracker.
- Add event data model and persistence.
- Add trigger condition model.
- Add action model routed through safe command paths.
- Add event evaluation engine.
- Prevent automation loops and unsafe repeated firing.
- Add event automation UI.
- Add automation visibility and logging.

Release 2 exit criteria:

- An engineer can build event automations without editing files by hand.
- Trigger conditions and actions are visible, reviewable, and testable before show use.
- Matching trigger conditions fire the intended actions.
- Safety checks apply to every action.
- Automation activity is visible enough to troubleshoot why an event fired, skipped, or was blocked.

## Issue Templates

Add these issue templates under `.github/ISSUE_TEMPLATE/`:

- `epic.yml`
- `feature_task.yml`
- `bug.yml`
- `config.yml`

The epic template should ask for:

- Summary.
- Milestone.
- Goal.
- Scope.
- Child issues.
- Acceptance criteria.
- Safety impact.
- Related docs or specs.

The feature/task template should ask for:

- Summary.
- Milestone.
- Problem or goal.
- Scope.
- Acceptance criteria.
- Safety impact.
- Testing and verification.
- Related docs or specs.

The bug template should ask for:

- Summary.
- Current behavior.
- Expected behavior.
- Reproduction steps.
- Safety impact.
- Logs or diagnostics.
- Testing and verification.

Each template should explicitly ask whether the issue touches LV1 state, scene recall, fade execution, fader writes, lockout, disconnect or reconnect handling, generation guards, stale state, or manual override behavior.

## Project-Local Opening Issue Skill

Add a project-local opencode skill at `.opencode/skills/opening-issues/SKILL.md`.

The skill should trigger when a user asks to open, create, file, or draft a GitHub issue for this repository. It should also trigger when future work, backlog work, roadmap work, milestone work, or acceptance criteria are being turned into an issue.

The skill should require agents to:

- Inspect existing GitHub milestones and related issues before creating a new issue when `gh` credentials are available.
- Ask one concise clarifying question if the requested issue lacks enough scope, milestone, or acceptance criteria to create a useful tracker item.
- Choose the right issue shape: epic, feature/task, bug, docs, chore, or release work.
- Assign or recommend the appropriate milestone.
- Include clear acceptance criteria.
- Include a safety impact section for any work touching LV1 state, scene recall, fade execution, fader writes, lockout, disconnect or reconnect handling, generation guards, stale state, or manual override behavior.
- Include testing or verification expectations appropriate to the issue.
- Link related docs, specs, plans, or existing issues when known.
- Avoid creating duplicate issues; if a likely duplicate exists, comment, update, or reference it instead of opening a new issue unless the user explicitly wants a separate issue.

The skill should make `gh issue create` the preferred creation path when repository credentials are available. If credentials are missing, it should produce a ready-to-paste issue title, body, labels, and milestone instead of claiming the issue was created.

Because opencode loads skills at startup, implementation should tell the user to restart opencode after adding the skill.

## Labels

Use lightweight labels for filtering:

- `type: epic`
- `type: feature`
- `type: bug`
- `type: chore`
- `type: docs`
- `area: backend`
- `area: frontend`
- `area: safety`
- `area: testing`
- `area: docs`
- `area: release`
- `priority: high`
- `priority: medium`
- `priority: low`
- `status: blocked`

The implementation should document the label set in `AGENTS.md` or in the issue template descriptions. It does not need to automate label creation unless that is the smallest practical way to create the initial GitHub tracker.

## Documentation Updates

`AGENTS.md` should stop instructing agents to read `docs/roadmap.md`. It should instead instruct agents to:

- Use `gh` to inspect GitHub Milestones and Issues for roadmap context.
- Read milestone descriptions for release scope, safety notes, and exit criteria.
- Treat issues as the source of truth for actionable work.
- File or reference GitHub issues for future ideas instead of adding roadmap items to repository docs.
- Continue reading `docs/architecture.md`, `docs/coding-conventions.md`, and `docs/lv1-osc.md` when relevant.

`docs/coding-conventions.md` and any other docs that mention adding future ideas to `docs/roadmap.md` should be updated to point to GitHub Issues instead.

## Implementation Notes

The initial GitHub tracker can be created with `gh issue create` and `gh api` for milestones if repository permissions are available. If credentials or permissions are unavailable, implementation should still add the repository issue templates and documentation updates, then provide the exact milestone and issue content for manual creation.

Before deleting `docs/roadmap.md`, implementation should update references in repository docs so no stale `docs/roadmap.md` guidance remains.

No Rust or frontend behavior changes are expected. Verification should use documentation/template-oriented checks such as searching for stale roadmap references and inspecting the generated issue template YAML.
