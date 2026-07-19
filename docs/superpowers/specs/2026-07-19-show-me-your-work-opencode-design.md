# OpenCode Show-Me-Your-Work Skill Design

**Date:** 2026-07-19

## Goal

Adapt the imported `show-me-your-work` skill to OpenCode and the Superpowers plan-execution workflows. Every written implementation plan gets a local, reviewable decision trail. The trail remains outside the working tree and is included in the pull request description when the user chooses to open a PR.

## Scope

- Remove Cursor-only frontmatter, commands, paths, transcript assumptions, and unavailable skill references.
- Make the skill discoverable for every written implementation plan, explicit decision-trail requests, unattended multi-phase work, and PR creation after plan execution.
- Integrate with `subagent-driven-development`, `executing-plans`, `todowrite`, `verification-before-completion`, `requesting-code-review`, and `finishing-a-development-branch` without duplicating their responsibilities.
- Store decision trails with the other local SDD handoff artifacts under Git metadata.
- Render the complete decision trail as a collapsed Markdown table in the PR description.
- Preserve the existing TSV schema and safe append behavior.

This work does not change application behavior, Rust code, frontend code, or the committed Superpowers implementation plan and specification locations.

## Activation And Responsibilities

The skill applies whenever an agent executes a written implementation plan through `subagent-driven-development` or `executing-plans`. It also applies when the user explicitly requests a decision trail, when work will run unattended across multiple phases, and when a branch with a plan decision trail is being finished through a PR.

The trail complements existing Superpowers state:

- `todowrite` remains the live task-status view.
- The SDD progress ledger remains the recovery map for completed tasks and commit ranges.
- The decision trail records review-worthy choices, pivots, blockers, completed-task checkpoints, and verification outcomes.
- Per-task and whole-branch reviewers remain responsible for code review.
- `verification-before-completion` remains responsible for fresh completion evidence.

The skill must not log routine file reads, searches, or obvious mechanical steps.

## Trail Location And Naming

The canonical trail path is:

```text
$(git rev-parse --git-path sdd)/<plan-stem>-decisions.tsv
```

`<plan-stem>` is the implementation plan filename without its `.md` extension. For example, executing `docs/superpowers/plans/2026-07-19-scene-recall-queue.md` uses:

```text
$(git rev-parse --git-path sdd)/2026-07-19-scene-recall-queue-decisions.tsv
```

This path places the trail beside SDD's progress ledger, task briefs, reports, and review packages. `git rev-parse --git-path sdd` is worktree-aware: a linked worktree receives a worktree-specific Git path. Because the trail is inside Git metadata rather than the working tree, it cannot be staged or committed accidentally.

The skill must not create a fallback decision trail in the repository root, `.audit/`, `docs/superpowers/`, or any other working-tree path. Resumed execution of the same plan appends to the existing trail.

## Trail Format And Logging

The existing six-column TSV schema remains unchanged:

```text
ts	phase	decision	why	evidence	result
```

One row represents one decision or checkpoint. Rows remain single-line and append-only. A correction gets a later superseding row rather than editing history.

The existing `scripts/log.sh` helper remains the canonical writer. It continues to:

- create the SDD directory and TSV header when needed;
- add an ISO 8601 UTC timestamp;
- strip tabs and line breaks from cells;
- protect spreadsheet users from formula execution; and
- append without rewriting previous rows.

At plan start, the controller derives the trail path from the plan filename. During execution it logs meaningful plan decisions and reviewed task checkpoints. Review completion rows point to durable evidence such as commit ranges, review packages, test output artifacts, or source paths.

## OpenCode Evidence Audit

The Cursor-specific transcript directory and `~/.cursor` guidance are removed. Before handoff, the controller audits the trail against evidence available in the active OpenCode run:

- tool results in the current conversation;
- commits and diff packages;
- test and verification output;
- task briefs, reports, reviewer reports, and the SDD progress ledger; and
- source, screenshot, trace, or artifact paths named by rows.

Every row must describe an action that occurred, and every evidence pointer must resolve or be marked inconclusive. Missing pivots or blockers are appended. Padding and aspirational claims are not allowed.

The review model or agent is identified only when OpenCode exposes that identity. The skill must not claim that a reviewer used a different model family unless the runtime actually confirms it. The independent task and whole-branch reviews required by the active Superpowers execution workflow satisfy the fresh-review requirement.

For a run that produced a trail, the final response retains an `Attention` section containing reviewer flags or `No flags`. It identifies the reviewer agent or model only when known.

## Pull Request Integration

When `finishing-a-development-branch` reaches the PR option for a plan-backed branch, the controller must audit and render the decision trail before creating the PR. The source TSV remains under the Git SDD path and is never staged or committed.

A new renderer helper converts the six TSV columns to a GitHub Markdown table and wraps the result in:

```markdown
<details>
<summary>Decision log</summary>

| Timestamp | Phase | Decision | Why | Evidence | Result |
|---|---|---|---|---|---|
| ... |

</details>
```

The rendered section follows the PR's normal summary, testing, and related-information sections. The renderer escapes Markdown table delimiters and backslashes so cell contents cannot break the table. The complete trail is included, not a summary.

If a written implementation plan was executed, a missing or malformed trail blocks PR creation until the trail is corrected. Work that genuinely did not execute a written plan may omit the decision-log section rather than inventing one.

## Skill Metadata And Composition

The OpenCode frontmatter contains only supported skill metadata. Its description starts with `Use when...` and names the concrete triggers: executing a written implementation plan, running unattended multi-phase work, explicitly requesting a decision trail, and creating a PR after plan execution. Cursor's `disable-model-invocation` key is removed.

The skill references Superpowers skills by name and lets each own its workflow. It does not restate their full procedures. References to unavailable `unslop` and `encode-lessons-in-structure` skills are replaced by direct plain-language and reproducible-evidence guidance.

## Error Handling

- If Git cannot resolve the SDD path, stop before plan execution and report the repository-context problem.
- If the log helper fails, do not claim the decision was recorded; resolve the write failure before continuing past the checkpoint.
- If an existing TSV has the wrong header or malformed rows, preserve it for diagnosis and stop appending until corrected.
- If evidence cannot be verified, record `INCONCLUSIVE` rather than a passing result.
- If Markdown rendering fails or the trail is missing for plan-backed work, do not create the PR until fixed.

## Testing And Verification

Skill behavior follows the `writing-skills` TDD workflow:

1. Run baseline subagent scenarios against the current imported skill and capture failures caused by Cursor paths, unsupported model assumptions, missing SDD integration, and absent PR embedding.
2. Update the skill minimally to address those observed failures.
3. Re-run equivalent fresh-context scenarios with the adapted skill and verify correct path derivation, logging decisions, evidence auditing, Superpowers composition, and PR handoff.

Helper verification includes:

- shell syntax validation for each helper;
- existing `log.sh` coverage for header creation, append-only writes, single-line sanitation, and spreadsheet formula protection;
- renderer coverage for valid TSV conversion, collapsed details markup, Markdown escaping, empty cells, malformed headers, and malformed rows; and
- an OpenCode discovery check confirming the local skill loads from `.opencode/skills/show-me-your-work/SKILL.md`.

No Rust test category applies because this change does not touch Rust behavior.

## Acceptance Criteria

- OpenCode discovers the adapted project-local skill without Cursor-only metadata.
- Every written implementation plan derives one worktree-aware decision trail under `$(git rev-parse --git-path sdd)`.
- The trail is never stored, staged, or committed in the working tree.
- The skill composes with both Superpowers plan-execution workflows and does not replace todos, progress ledgers, reviews, or verification.
- Trail auditing relies on available OpenCode and SDD evidence without assuming Cursor transcript paths.
- Reviewer identity and model-family claims are made only when confirmed.
- Choosing the PR completion option embeds the complete trail as a valid collapsed Markdown table after the normal PR content.
- Plan-backed PR creation stops on a missing, malformed, unaudited, or unrenderable trail.
- Non-plan work does not fabricate a decision trail.
- Helper tests and OpenCode discovery verification pass.
