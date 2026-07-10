---
name: building-ui
description: Use when changing frontend UI, React components, styling, Storybook stories, UI state projection, settings forms, or user interaction flows in this LV1 fade utility.
---

# Building UI

## Overview

The UI presents backend-owned app state and captures user intent. Keep frontend logic thin; functionality, validation, and safety policy belong in the backend.

## When To Use

- Changing React or TypeScript code under `ui/`.
- Adding or changing components, styling, forms, settings UI, or user flows.
- Adding or changing Storybook stories or Storybook interaction tests.
- Displaying backend-owned state or sending user commands to the backend.

## Core Rules

- Frontend code lives under `ui/`; do not assume a root `src/` frontend.
- Preserve the existing design language unless the task is to redesign it.
- Define reusable fonts, colors, spacing, borders, and interaction states as Tailwind/CSS theme variables when a value is reusable.
- Avoid hard-coded Tailwind values when a reusable token is appropriate.
- Keep backend-owned state projected from backend snapshots.
- Do not bypass `app-status-changed` for backend-owned state.
- Use full-object replacement for settings updates unless the backend API explicitly exposes a narrower command.
- Prefer thin frontend logic and rely on backend implementation for real app functionality, validation, and safety behavior.

## Storybook

- Maintain Storybook for components or flows with meaningful visual states.
- Add or update stories when UI changes introduce new states, variants, empty states, error states, or disabled/safety states.
- Keep stories representative of projected backend state instead of inventing alternate frontend-only behavior.
- Use Storybook interaction tests when the story demonstrates important UI behavior.

## Frontend Boundary Guide

| Need | Preferred owner |
|---|---|
| Formatting projected values for display | Frontend |
| Local input draft state | Frontend |
| Persisting settings or show data | Backend command |
| Validating LV1/fade/scene safety | Backend |
| Deciding whether an operation is safe | Backend |
| Displaying disabled, blocked, or error state | Frontend from projected backend state |

## Common Mistakes

- Duplicating backend state machines in React state.
- Implementing safety decisions in the UI because it is convenient.
- Adding one-off colors or spacing instead of using theme tokens.
- Changing visible component states without updating Storybook.
- Sending narrow settings patches when the backend expects full-object replacement.
