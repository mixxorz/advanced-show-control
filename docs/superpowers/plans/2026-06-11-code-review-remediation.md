# Code Review Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the remaining `CODE_REVIEW.md` remediation items in the order requested by the pasted remaining-items list.

**Architecture:** Keep the existing actor/bus boundaries. Prefer small safety-enforcing API changes over broad refactors, and make each remediation independently testable before moving to the next bucket.

**Tech Stack:** Rust core crate, Tauri Rust shell crate, React/TypeScript UI, `cargo nextest`, `npm run typecheck`, `npm run build`.

---

## Tasks

1. Move blocking connect discovery off async runtime.
2. Harden generation-checked recall fade dispatch.
3. Replace misleading tests and add missing fade coverage.
4. Strengthen show-file mapping and bound backups.
5. Eliminate silent scene-id fallbacks.
6. Small cleanup and visible lag handling.
7. Update runtime documentation.
8. Small UI quality basket.
9. Full verification.

Use the full plan from the controller prompt for exact task steps during execution.
