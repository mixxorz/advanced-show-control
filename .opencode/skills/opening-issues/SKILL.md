---
name: opening-issues
description: Use when opening, creating, filing, drafting, or updating GitHub issues for this repository, including backlog, roadmap, milestone, epic, acceptance criteria, or future-work tracking.
---

# Opening Issues

## Overview

GitHub Milestones and Issues are the source of truth for roadmap and actionable work. Open issues that are specific, safely scoped, and connected to the correct milestone instead of creating duplicate or vague tracker items.

## When To Use

- The user asks to open, create, file, draft, or update a GitHub issue.
- Roadmap, backlog, milestone, future work, acceptance criteria, or release scope needs to become an issue.
- Existing docs, specs, plans, review findings, or user notes need to be tracked in GitHub.

## Required Workflow

1. Inspect existing GitHub milestones and related issues with `gh` when credentials are available.
2. If scope, milestone, or acceptance criteria are too vague for a useful issue, ask one concise clarifying question before creating it.
3. Choose the issue shape: epic, feature/task, bug, docs, chore, testing, or release work.
4. Assign or recommend the milestone.
5. Include acceptance criteria that make the issue closable.
6. Include safety impact, especially for LV1 state, scene recall, fade execution, fader writes, lockout, disconnect/reconnect handling, generation guards, stale state, or manual override behavior.
7. Include testing or verification expectations.
8. Link related docs, specs, plans, commits, or issues when known.
9. Avoid duplicates. If a likely duplicate exists, comment, update, or reference it instead of opening a new issue unless the user explicitly wants a separate issue.

## Creation Rules

| Situation | Action |
|---|---|
| `gh` is authenticated and the request is clear | Use `gh issue create` with title, body, labels, and milestone. |
| `gh` is unavailable or not authenticated | Provide a ready-to-paste title, body, labels, and milestone. |
| A likely duplicate exists | Do not create a new issue by default; reference or update the existing issue. |
| User asks for an epic | Use `type: epic` and include linked child issue checklist items. |
| Safety-critical behavior is involved | Add `area: safety` and explicit safety constraints. |

## Issue Body Shape

Use this structure unless an issue template or user request requires a better fit:

```markdown
## Summary

## Scope

## Acceptance Criteria
- [ ]

## Safety Impact

## Testing And Verification

## Related Docs Or Issues
```

For bugs, use:

```markdown
## Summary

## Current Behavior

## Expected Behavior

## Reproduction Steps
1.
2.
3.

## Safety Impact

## Logs Or Diagnostics

## Testing And Verification
```

## Common Mistakes

- Creating an issue without checking existing issues or milestones.
- Filing broad roadmap text as a single unclosable issue.
- Omitting safety impact for scene recall, fade, or LV1-state work.
- Claiming an issue was created when `gh` failed or credentials were missing.
- Recreating roadmap state in repository docs instead of GitHub Milestones and Issues.
