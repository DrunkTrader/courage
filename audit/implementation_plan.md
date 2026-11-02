# Implementation plan

## Critical reliability follow-up (supersedes previous code-complete labels)

- Implemented and locally tested: single owner, pipes/readers, session state, graceful deadlines, forced fallback and retryable cleanup. Windows atomic Job Object creation cross-compiles; native job/tree/crash/sharing tests await Windows execution.
- Implemented: shared window Close/tray Quit/ExitRequested coordinator, immediate admission barrier, off-UI blocking commands and explicit Hide. Actual Windows close/logoff/crash checks remain open.
- Implemented and deterministic tests pass: locale-specific grammar, final-result/confidence/dedup/expiry gating, draft text and no-save microphone test, diagnostics. Real microphone accuracy/CPU/RAM/latency remain open.
- Implemented: native general autofill off plus input hints. Chromium 1412 source identified separately; runtime reproduction and packaged editing/autofill checks remain open.
- Verified on Linux: 10 real-FFmpeg start/stop plus 10 start/save/stop cycles. Windows PID-scoped automation supplied; Windows runtime acceptance is blocked on a Windows host. Follow `test_results.md` for final build/check outcomes.

## Historical plan for the previous implementation

The checkboxes below record implementation history, not proof of Windows runtime acceptance.

## Phase 1 — Baseline and audit — verified

- [x] Inspect workspace, Tauri, frontend, capture, store, assets, and docs.
- [x] Run formatter, Rust check/tests/clippy, and frontend build.
- [x] Record findings in `CODEBASE_REVIEW.md`.

## Phase 2 — Reliability and data truth — code complete; Windows verification pending

- [x] Add backward-compatible persisted capture settings with FPS, resolution, quality, cursor, audio, voice phrases, theme, and hotkey fields.
- [x] Use unique per-session segment directories and explicit stop/reap/cleanup transitions.
- [x] Add backend library reconciliation and application delete/reveal commands.
- [x] Add lifecycle/error events and actionable user messages.
- Acceptance: repeated start/stop does not reuse locked intermediate files; library omits missing files after reconciliation.

## Phase 3 — Voice and hotkey controls — code complete; Windows verification pending

- [x] Implement normalized wake phrase + command phrase matching with explicit wake-only mode and cooldown.
- [x] Persist selected recording input and expose recognition diagnostics/test state; native System.Speech remains bound to Windows default input.
- [x] Add supported hotkey capture UI; document plugin support for physical numpad identities.
- Acceptance: wake phrase alone does not save unless wake-only is enabled; one utterance produces at most one save.

## Phase 4 — Capture settings — code complete; Windows verification pending

- [x] Wire FPS, resolution, capture source, cursor, quality, codec/encoder preference, and audio parameters into FFmpeg arguments.
- [x] Validate settings on the backend and restart only when active capture settings change.
- Acceptance: changed settings are visible in active status and change the generated FFmpeg command.

## Phase 5 — UI and themes — code complete; runtime verification pending

- [x] Replace hard-coded dark tokens with shared light/dark/rusty tokens.
- [x] Redesign Library/Capture/Settings into compact reusable controls and accessible states.
- [x] Disable browser context menus outside editable/native controls and browser accelerator actions.
- Acceptance: theme persists across restart; production app background/cards do not show browser context menu.

## Phase 6 — Branding and packaging — code complete; Windows package verification pending

- [x] Wire existing ICO/PNG assets into Tauri bundle/window/tray configuration.
- [x] Replace Unicode control glyphs with a consistent inline icon set.
- Acceptance: debug/package builds reference the existing Courage icon assets.

## Phase 7 — Benchmarks and verification — harness complete; Windows execution pending

- [x] Add PowerShell benchmark harness and JSON/CSV result output.
- [ ] Run Linux-verifiable media checks and Windows manual matrix when a Windows host is available.
- [x] Update progress, test results, and benchmark README with exact evidence.
