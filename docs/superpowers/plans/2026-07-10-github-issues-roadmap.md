# GitHub Issues Roadmap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the in-repo roadmap with GitHub Milestones and Issues, add issue templates, and add a project-local skill for opening issues.

**Architecture:** GitHub owns roadmap and actionable tracker state through milestones and issues. Repository docs point agents to GitHub, while in-repo safety, architecture, and coding conventions remain the durable project references. `.github/ISSUE_TEMPLATE/` defines issue shape, and `.opencode/skills/opening-issues/SKILL.md` guides future agents when filing issues.

**Tech Stack:** GitHub CLI, GitHub Issues, GitHub Milestones, Markdown docs, GitHub issue forms YAML, opencode project skills.

## Global Constraints

- Execute inline on `main`; do not dispatch subagents.
- Modify GitHub state directly when `gh` credentials are available.
- Delete `docs/roadmap.md`; do not recreate a roadmap mirror elsewhere.
- Fold Cue Lists into the MVP milestone.
- Remove External Control and Stream Deck from current roadmap tracking.
- Do not add GitHub Projects automation, bots, or sync scripts.
- No Rust or frontend behavior changes are expected.

---

### Task 1: Add Repository Issue Templates

**Files:**
- Create: `.github/ISSUE_TEMPLATE/epic.yml`
- Create: `.github/ISSUE_TEMPLATE/feature_task.yml`
- Create: `.github/ISSUE_TEMPLATE/bug.yml`
- Create: `.github/ISSUE_TEMPLATE/config.yml`

**Interfaces:**
- Consumes: approved issue template requirements from `docs/superpowers/specs/2026-07-10-github-issues-roadmap-design.md`.
- Produces: GitHub issue form templates for epics, feature/tasks, and bugs.

- [ ] **Step 1: Create issue template files**

Use GitHub issue forms with required fields for milestone, scope, acceptance criteria, safety impact, and verification.

- [ ] **Step 2: Verify template files exist**

Run: `test -f .github/ISSUE_TEMPLATE/epic.yml && test -f .github/ISSUE_TEMPLATE/feature_task.yml && test -f .github/ISSUE_TEMPLATE/bug.yml && test -f .github/ISSUE_TEMPLATE/config.yml`

Expected: command exits 0.

- [ ] **Step 3: Commit**

Run: `git add .github/ISSUE_TEMPLATE && git commit -m "docs: add github issue templates"`

---

### Task 2: Add Project-Local Opening Issues Skill

**Files:**
- Create: `.opencode/skills/opening-issues/SKILL.md`

**Interfaces:**
- Consumes: GitHub milestone/issue tracker conventions from the spec and issue templates from Task 1.
- Produces: a project-local opencode skill named `opening-issues`.

- [ ] **Step 1: Create the skill**

Add frontmatter with `name: opening-issues` and a trigger-focused description. Include guidance to inspect milestones/issues, avoid duplicates, ask one clarifying question when scope is missing, include safety impact, include verification, and use `gh issue create` when authenticated.

- [ ] **Step 2: Verify frontmatter and path**

Run: `test -f .opencode/skills/opening-issues/SKILL.md && grep -q "name: opening-issues" .opencode/skills/opening-issues/SKILL.md`

Expected: command exits 0.

- [ ] **Step 3: Commit**

Run: `git add .opencode/skills/opening-issues/SKILL.md && git commit -m "docs: add issue opening skill"`

---

### Task 3: Replace Roadmap References In Repo Docs

**Files:**
- Modify: `AGENTS.md`
- Modify: `docs/coding-conventions.md`
- Delete: `docs/roadmap.md`

**Interfaces:**
- Consumes: GitHub roadmap source-of-truth decision from the spec.
- Produces: repository guidance that points to GitHub Milestones and Issues instead of `docs/roadmap.md`.

- [ ] **Step 1: Update docs**

Remove `docs/roadmap.md` guidance from `AGENTS.md` and `docs/coding-conventions.md`. Add GitHub Milestones and Issues as the roadmap and actionable tracker source of truth.

- [ ] **Step 2: Delete roadmap**

Delete `docs/roadmap.md`.

- [ ] **Step 3: Verify stale guidance is gone**

Run: `rg "docs/roadmap\.md|Add future ideas to the appropriate release section" AGENTS.md docs/coding-conventions.md`

Expected: no matches.

- [ ] **Step 4: Commit**

Run: `git add AGENTS.md docs/coding-conventions.md docs/roadmap.md && git commit -m "docs: move roadmap guidance to github"`

---

### Task 4: Create Labels, Milestones, And Issues In GitHub

**Files:**
- No repository file changes expected.

**Interfaces:**
- Consumes: label, milestone, and issue lists from the spec.
- Produces: GitHub labels, milestones, and initial issues.

- [ ] **Step 1: Identify repository**

Run: `gh repo view --json nameWithOwner --jq .nameWithOwner`

Expected: prints the target GitHub repository.

- [ ] **Step 2: Create or update labels**

Use `gh label create` or `gh label edit` for the labels from the spec.

- [ ] **Step 3: Create or update milestones**

Use `gh api` to create or update `MVP: Scene Fades + Cue Lists` and `Release 2: Event Automation Engine` with descriptions from the spec.

- [ ] **Step 4: Create initial issues**

Use `gh issue create` for the MVP and Release 2 initial issues from the spec. Apply labels and milestones. Do not create External Control or Stream Deck issues.

- [ ] **Step 5: Verify GitHub state**

Run: `gh issue list --limit 100 --state open --json number,title,milestone,labels`

Expected: output includes MVP and Release 2 issues with milestones and labels.

---

### Task 5: Final Verification

**Files:**
- No new files unless verification exposes docs issues.

**Interfaces:**
- Consumes: completed Tasks 1-4.
- Produces: verified working tree, committed docs/config changes, and created GitHub tracker state.

- [ ] **Step 1: Check for stale roadmap references in primary docs**

Run: `rg "docs/roadmap\.md|Add future ideas to the appropriate release section" AGENTS.md docs/coding-conventions.md`

Expected: no matches.

- [ ] **Step 2: Check Git status**

Run: `git status --short`

Expected: no uncommitted changes.

- [ ] **Step 3: Inspect recent commits**

Run: `git log --oneline -5`

Expected: includes commits from Tasks 1-3.

- [ ] **Step 4: Note opencode restart requirement**

Record in the final response that opencode must be restarted to load the new project-local skill.

## Self-Review

- Spec coverage: Tasks cover deleting `docs/roadmap.md`, updating `AGENTS.md` and coding conventions, adding issue templates, adding the local opencode skill, adding labels, creating milestones, creating issues, folding Cue Lists into MVP, and omitting External Control/Stream Deck.
- Placeholder scan: No `TBD`, `TODO`, or fill-in placeholders remain.
- Type consistency: File paths and names match the approved spec.
