Status: DONE

Commits created:
- pending

Files changed:
- ui/src/sessionTitle.ts
- ui/src/sessionTitle.test.ts
- ui/src/AppRuntime.tsx
- ui/src/AppRuntime.test.tsx
- ui/src/App.tsx

Tests run:
- `npm run test -- --run src/sessionTitle.test.ts src/AppRuntime.test.tsx` (pass)
- `npm run typecheck` (pass)

Self-review notes:
- Window title updates are projection-driven from `AppViewState.showFileName` and `AppViewState.showFileDirty`.
- The title formatter is isolated and covered by unit tests.
- The AppRuntime title effect only calls the injected service and reports service errors through `commandError`.

Concerns:
- `AppRuntime.test.tsx` contained stale assertions for current UI labels; I updated those assertions to match the current shell so the focused suite passes.
