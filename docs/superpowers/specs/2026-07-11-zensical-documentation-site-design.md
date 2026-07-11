# Zensical Documentation Site Design

## Purpose

Create a public, versioned documentation site for Advanced Show Control that satisfies GitHub issues #34 and #24. The site is for live sound engineers using the application and documents the current application screens and workflows without publishing internal architecture or contributor documentation.

The site will use Zensical, publish through GitHub Pages, and be available at `https://mitchel.me/advanced-show-control/`.

## Scope

The first release includes:

- A lightly branded Zensical site and landing page.
- User documentation for installation, connection, sessions, scene fades, cue lists, settings, logs, safety behavior, and troubleshooting.
- Screen-oriented guides that explain what each visible control and state means.
- Selective screenshots copied from the visual regression test assets.
- Versioned documentation with `stable` and `latest` aliases.
- Automated validation and GitHub Pages deployment.

The site does not publish the existing internal documents under `docs/`, such as architecture, protocol, coding convention, or implementation-planning documents. It does not add developer or contributor documentation.

## Site Architecture

Add a dedicated `site/` directory containing:

- `zensical.toml` for site configuration and navigation.
- User-facing Markdown source files.
- Site-specific styles and branding assets.
- Copies of selected visual regression screenshots used by the guides.

Keeping the public site separate from `docs/` prevents internal engineering material from entering the public navigation and gives the user documentation an independent information architecture.

The site uses Zensical's modern theme with restrained Advanced Show Control branding. Customization should reuse suitable application colors and logo assets, provide readable typography, and create a polished landing page without replacing the theme's core navigation or content layout.

The canonical URL is `https://mitchel.me/advanced-show-control/`. Repository links point to the Advanced Show Control GitHub repository.

## Information Architecture

### Home

The landing page explains what Advanced Show Control does, establishes that LV1 remains the source of truth, states the application's current maturity, and routes users directly to installation and quick-start documentation.

### Getting Started

Getting Started covers:

- System and LV1 requirements.
- Installation and current packaging or platform limitations.
- First launch.
- The connection dialog.
- LV1 discovery and manual connection.
- Connection states and common connection failures.
- Creating the first session.
- Configuring and trying one basic scene fade.

### Application Shell

The Application Shell guide covers:

- Top-level navigation.
- Connection status and controls.
- Bottom status indicators.
- Lockout.
- Abort All.
- Window title and session state.
- File menu behavior.
- New, Open, Save, and Save As workflows.
- `.ascs` session files.
- Dirty-session behavior and current limitations.

### Scenes Screen

The Scenes guide covers:

- Scene list states and selection.
- Linking or capturing an LV1 scene.
- Selected-scene actions.
- Fade duration.
- Parameter scope toggles.
- Channel scope controls and grid.
- Stored targets.
- Scene recall and fade behavior.
- Related confirmation and overwrite dialogs.
- Empty, disconnected, mismatch, blocked, and error states.

### Cue Lists Screen

The Cue Lists guide covers:

- Active cue-list selection.
- Creating, renaming, deleting, and managing lists.
- Adding, removing, and reordering entries.
- Selected and cued states.
- Cue recall controls.
- Keyboard operation.
- Missing or mismatched scene states.
- Blocked and error states.

### Settings Screen

The Settings guide explains every visible setting, its valid values, persistence behavior, and when changes take effect. Settings displayed by the application but not yet wired to runtime behavior must be identified plainly rather than described as functional.

### Console And Logs Screen

The Console and Logs guide covers:

- Operational log messages.
- Log severity meaning.
- The distinction between frontend-visible operational messages and diagnostic logs.
- Diagnostic log locations.
- Information users should include in a bug report.

### Troubleshooting

Troubleshooting is a symptom-based index. It directs users to the authoritative screen guide and relevant state explanation instead of duplicating full procedures. Initial topics include discovery and connection failures, scene mismatches, blocked recalls, fades that do not start, session-file problems, and diagnostic collection.

### Reference

Reference content includes keyboard shortcuts and application terminology.

## Guide Pattern

Each screen guide follows a consistent structure:

1. The screen's purpose.
2. An annotated overview where a screenshot improves comprehension.
3. A control-by-control explanation.
4. The common workflow for that screen.
5. Safety notes beside the controls and actions they govern.
6. Empty, disconnected, blocked, and error states.
7. Links to related troubleshooting entries.

Safety is not a standalone documentation section. Guidance for lockout, Abort All, manual override, disconnects, stale or unsafe state, exact scene matching, and blocked recalls appears at the point where the user encounters the relevant action or status. The text must not overstate guarantees or supported workflows.

## Writing Standard

Use a formal, precise technical-manual voice throughout the site.

- Use imperative language for procedures, such as "Select **Connect**."
- Use impersonal declarative language for reference descriptions, such as "The status bar displays the current LV1 connection state."
- Prefer short, literal sentences and exact user-interface labels.
- Introduce one concept at a time.
- Avoid promotional, conversational, playful, or vague wording.
- State prerequisites before actions and expected results after them.
- Distinguish requirements, recommendations, notes, warnings, and limitations.
- Use **must** only for required actions or safety constraints, **should** for recommendations, and **may** for optional behavior.
- Describe current behavior only. Identify unavailable or incomplete behavior explicitly.
- Refer to Waves eMotion LV1 as **LV1** after its first use.
- Refer to the product as **Advanced Show Control** or **the application**, never "we."
- Keep safety notes factual and beside the relevant control or procedure.
- Do not use vague reassurance or overstate safety guarantees.
- Use numbered steps for sequential procedures, bullets for nonsequential facts, and tables for compact control references.
- Format exact user-interface labels in bold, file names and paths as code, and user-entered values as code.

Example:

> Select **Lockout** before editing scene fade settings during show operation. Lockout prevents application-initiated recalls while enabled. It does not disable controls in LV1.

## Screenshot Strategy

Use existing visual regression test screenshots as the source for application imagery. Copy selected screenshots into the `site/` directory so each documentation version contains immutable assets matching that source revision.

Only include screenshots that materially improve understanding. Screenshots may be cropped or annotated for documentation, while the original visual regression assets remain the UI source of truth. The docs copies are intentionally independent so publishing an old release does not depend on assets from another revision or generated test output.

## Versioning Model

Use Zensical's documented `mike` integration and enable the Zensical version selector.

- `stable` points to documentation from the latest stable release tag.
- `latest` points to documentation built from `main`.
- The site root redirects to `stable` by default.
- Named stable release versions remain available in the version selector.
- Development documentation is visibly identified as `latest` so users can distinguish it from released behavior.

The deployment process writes versions to the shared GitHub Pages branch. Deployment jobs must be serialized to prevent concurrent `mike` operations from racing or overwriting each other.

## Publishing Flow

GitHub Actions provides separate validation and deployment responsibilities:

- Relevant pull requests and pushes perform a clean Zensical build from the current checkout.
- Pushes to `main` publish a development version and update the `latest` alias.
- Stable release publication builds documentation from the release revision, publishes that named release version, and updates the `stable` alias.
- The `stable` alias is configured as the root default.

Zensical and the Zensical-compatible `mike` fork must be pinned in automation so site output does not change unexpectedly. A failed build or deploy must leave the previously published site intact and expose the failure through GitHub Actions.

GitHub Pages serves the repository project site. DNS and Pages configuration expose it at `https://mitchel.me/advanced-show-control/`.

## Accuracy And Maintenance

The `latest` documentation describes behavior on `main`. A release tag preserves the site source and copied screenshots for that release, allowing `stable` to remain aligned with released behavior.

Documentation should be checked against:

- Frontend interaction tests as the primary source for user-visible behavior.
- Current React components and application shell structure.
- Storybook stories and visual regression states.
- Implemented backend behavior and Rust tests only where they clarify safety, persistence, or runtime details that the frontend does not fully express.

When frontend and backend sources emphasize different details, the guide should lead with the behavior visible to the user. Backend implementation details should appear only when they affect an observable result, limitation, or safety constraint.

Pages must state current limitations directly. Planned behavior must not be presented as available behavior.

## Verification

Implementation is complete when:

- A clean local Zensical build succeeds.
- Navigation, internal links, assets, canonical URLs, and the configured project base path resolve correctly.
- The version selector identifies `stable`, `latest`, and retained release versions correctly.
- The root URL redirects to `stable`.
- The `main` deployment path updates only the development version and `latest` alias.
- The stable release path publishes the release version and updates only the `stable` alias.
- Deployment jobs cannot run concurrently against the shared Pages branch.
- The published root, `stable`, and `latest` URLs are inspected after deployment when repository settings permit publication.
- User procedures and safety notes are reviewed against implemented behavior.

## Non-Goals

- Publishing internal architecture, protocol, contributor, or implementation-planning documentation.
- Building a separate custom documentation frontend.
- Documenting the future Event Automation release before its behavior exists on the documented revision.
- Creating a screenshot for every control or state.
- Changing application runtime behavior.
