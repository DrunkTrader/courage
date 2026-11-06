# Progress

## UI polish, sidebar layout and Windows startup — 2026-11-06

- Scope: non-selectable utility chrome, a viewport-locked sidebar, GUI-only production startup, and a rebuilt voice-trigger testing panel. Root causes recorded in `audit/ui-polish-review.md` before any code changed. Capture, FFmpeg lifecycle, library reconciliation, hotkeys, voice matching and theming were not touched.
- Terminal window on launch: `main.rs` carried no `windows_subsystem` attribute, so the binary linked against the console subsystem. Child processes were already clean (`windows_process.rs:177` passes `CREATE_NO_WINDOW`; `lib.rs:1947` sets `creation_flags(0x08000000)`), so they were never the cause. Fixed with `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`, which keeps the console for `tauri dev` and `diagnostic_event` stderr.
- Sidebar: `.app-shell` had `min-height: 100vh` with no viewport cap, so the grid row grew to the full document height and `.sidebar-foot { margin-top: auto }` pushed `Hide to tray` to the bottom of the *document*, below the fold, while the whole page scrolled as one block. Fixed structurally — the shell is now `100dvh` with `overflow: hidden`, and a new `.workspace-scroll` grid cell owns the scrolling. No fixed offsets or `position: fixed` hacks. The `≤900px` stacked breakpoint explicitly releases the lock so the document scrolls again.
- The scroller is the grid cell rather than the centred `.workspace` box, so the scrollbar stays at the window edge instead of being inset by up to ~140px on a maximized 1920px display.
- Text selection: no selection policy existed at all. `user-select: none` now applies to `.app-shell`, re-enabled for `input, textarea, select, [contenteditable='true'], code, .clip-path` — the same escape list the existing context-menu suppression at `App.tsx:558` already used, so clip paths and engine diagnostics remain copyable.
- Voice test panel: the `<dl>`/`<dt>`/`<dd>` and `<p role="alert">` had **zero** CSS rules and fell back to UA defaults — 16px text and a 40px `dd` indent next to 9–11px designed elements — which was the "oversized disconnected text". Results were also flattened into one `·`-joined prose string, so nothing could be aligned or coloured. Replaced with two labelled cards (`Test phrase matching` / `Test microphone`), structured `VoiceDecision` state instead of a pre-formatted sentence, aligned label/value rows, semantic status dots, and engine diagnostics in a `<details>` section. No diagnostic field was dropped; the Rust commands and the no-save guarantee are unchanged.
- Verified headlessly against the real built frontend (Chromium, same-origin probe harness, since Tauri cannot run without GTK here): exactly one scroll container (`workspace-scroll`), sidebar and footer move 0px when the content scrolls 800px, all three nav items plus `Hide to tray` visible at both 1200x760 and the configured 960x640 minimum, `user-select` resolving to `none` on headings/nav and `text` on settings inputs, and all three themes rendering. Probe files were deleted afterwards.
- Still open and Windows-only: launching the packaged NSIS/MSI build on a real Windows desktop to confirm no console window appears in practice, and exercising the microphone card against a real System.Speech recognizer.

## Critical lifecycle follow-up — in progress

- Baseline `903fa83`; working tree initially clean. User reports Windows sharing violation 32, live FFmpeg microphone ownership, low-confidence unrelated transcripts, Saved Info popup, and Chromium error 1412.
- Confirmed by code inspection: close-to-tray masquerades as exit; no process-tree ownership; kill-only termination; detached pipe readers; cleanup errors discard the session; speech uses DictationGrammar/default locale; phrase test uses persisted rather than draft configuration.
- Precise Windows locking process/path is not known from the supplied log. Linux cannot reproduce Windows sharing semantics or verify microphone privacy state. Add path/PID/session evidence and Windows-specific tests.
- Sequence: managed process/lifecycle and exit → microphone/voice → native autofill → reproducible verification and audit correction.
- Implemented managed media/speech children, bounded pipes, joined readers, graceful deadlines, Windows atomic Job Object assignment, explicit capture states and retryable exact-path cleanup. Window Close and tray Quit now coordinate shutdown; Hide is explicit.
- First ownership test run exposed a fixture stdout-prefix problem (libtest prints its test name without a newline); corrected the fixture. Second run: 12 desktop tests passed, 3 explicit fixtures/integration tests ignored by default. Windows-target compilation passed; runtime remains unverified.
- Microphone ownership: FFmpeg DirectShow and System.Speech default-device input are separate consumers. Both now use managed children; speech explicitly disposes input on graceful stop. No evidence yet supports a shared-audio architecture rewrite.
- Replaced dictation with culture-specific command grammar; added final-result/confidence/duplicate/expiry gating, draft-based text tests, bounded no-save microphone testing and engine diagnostics. Native general autofill disabled using supported Tauri configuration; packaged Windows UI verification pending.
- Real FFmpeg validation passed: 10 start/stop plus 10 start/save/stop cycles (17.60 s test execution); all session directories removed, ten saved clips retained/probed. Real `-nostdin` forced-fallback check also passed (0.12 s).
- Final workspace run: 19 unit tests passed (15 desktop, 3 core, 1 store). Format/check/native clippy/Windows-target all-target clippy and media validation passed. Windows-only native tests compile but have not run.
- Browser smoke passed using mocked Tauri IPC: draft command handling, safe test action, keyboard copy/paste, Tab, three themes, no page errors. PowerShell syntax and unavailable-engine error protocol passed on Linux. These are not Windows microphone/WebView2 results.
- Added PID/session-scoped interactive Windows automation and real-microphone/manual acceptance protocols. Windows runtime and production installer verification remain open; no claim that the original sharing-lock holder or hardware accuracy has been verified.
- Windows GNU optimized cross-build passed through Tauri CLI (`--no-bundle`), producing a PE32+ x86-64 executable. Native MSVC/MSI/NSIS packaging and running it on Windows remain unverified. Before-build command now rebuilds frontend assets rather than packaging a stale dist directory.
- Final review fixed a queued-start/stop ordering race: a stop generation invalidates an older start even when Stop wins the state lock and finishes first. Regression test and full workspace rerun passed. A warm cross-build exceeded an overly short 120-second tool deadline; the final rebuild without that deadline passed in 2m44s. Final native/Windows-target clippy, formatting and diff checks passed.

## 2026-11-02

- Baseline established on clean `main` at `3714105`.
- Rust format/check/tests/clippy and frontend build passed.
- Audit findings recorded; Windows hardware/runtime remains blocked in this environment.
- Added backward-compatible capture settings and FFmpeg wiring for FPS, scaled resolution, quality, encoder preference, cursor, audio, and replay length.
- Added unique capture session directories, child reaping, cleanup, voice cooldown/matching tests, library reconciliation, external MP4 import, application deletion, and library events.
- Rebuilt the frontend with accessible shared controls, Light/Dark/Rusty themes, hotkey recording, voice phrase testing, inline icons, and browser-action suppression.
- Wired existing Courage icons into Tauri bundle configuration and added a Windows PowerShell benchmark harness.
- Added a manual Windows verification matrix, baseline/post-fix comparison script, structured diagnostics, stable external-file reconciliation, and deterministic media benchmark evidence.
- Added native WebView2 context-menu suppression, hardware encoder capability probing/fallback, capture-operation busy states, safer settings replacement, and cleanup of failed clip exports.
- Added transient Windows speech recognition result/confidence diagnostics without persisting transcripts; the configured default requires the full wake-plus-command phrase.
- Final Linux and Windows-target Rust checks, workspace tests/clippy, frontend build, Tauri debug build, and diff checks pass; Windows hardware/runtime remains pending.
- Remaining evidence is Windows-only: real FFmpeg start/stop cycles, WebView2 context menu behavior, speech/hotkey/tray behavior, packaging, and resource measurements.
