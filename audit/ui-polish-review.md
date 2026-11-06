# UI Polish, Sidebar Layout & Windows Startup — Review and Plan

Date: 2026-11-06
Scope: `apps/desktop/src/styles.css`, `apps/desktop/src/App.tsx`, `apps/desktop/src-tauri/src/main.rs`

## Environment actually in use (verified, not assumed)

| Thing | Value | Source |
|---|---|---|
| Tauri | 2.8 (`$schema: schema.tauri.app/config/2`) | `src-tauri/Cargo.toml`, `tauri.conf.json` |
| Frontend | React 19 + Vite 7, plain CSS, no Tailwind/Prettier/ESLint | `apps/desktop/package.json` |
| CSS architecture | one hand-written stylesheet, CSS custom properties per `:root[data-theme]` (dark/light/rusty) | `src/styles.css:1-66` |
| UI structure | single-file React app; shell is `<main class="app-shell">` → `<aside class="sidebar">` + `<section class="workspace">` | `src/App.tsx:565-584` |
| CSP | `style-src 'self'` — **no inline `style=` attributes allowed**; all styling must stay in the stylesheet | `tauri.conf.json` |
| Bundle targets | `nsis`, `msi` (Windows) | `tauri.conf.json` |

Note: the task referenced attached screenshots; `screenshots/` is empty in this worktree, so root causes
below were derived from the code itself and from rendered-layout reasoning, not from the images.

## Root causes

### 1. Text selection — no selection policy exists at all

`grep user-select` over `styles.css` returns nothing. The app never opts out of text selection, so the
WebView applies normal document behaviour and a click-drag selects headings, nav labels, and diagnostics.
`App.tsx:558` already suppresses the browser context menu with exactly the right escape list
(`input, textarea, select, [contenteditable="true"]`) — the selection fix should mirror that list rather
than invent a second policy.

**Fix:** `user-select: none` on `.app-shell`, re-enabled on editable controls plus `code` (clip paths and
engine diagnostics are worth copying). Both prefixed and unprefixed, because macOS WKWebView still needs
`-webkit-user-select`.

### 2. Sidebar — the grid row is as tall as the document, not the viewport

`styles.css:77` sets `.app-shell { min-height: 100vh; display: grid; grid-template-columns: 218px minmax(0,1fr) }`
and `styles.css:96` sets `.workspace { min-height: 100vh }`. Nothing constrains the grid to the viewport, so
when the workspace content is taller than the window the **grid row grows with it** and the sidebar cell
stretches to the full document height. `.sidebar-foot { margin-top: auto }` (`styles.css:90`) then pushes
`Hide to tray` and the `Local only` pill to the bottom of the *document*, which is far below the fold. The
whole page scrolls as one block, so the sidebar scrolls away too.

This is a structural bug, not an offset bug: the sidebar is doing exactly what `margin-top:auto` asks, in a
box that is the wrong height.

**Fix:** lock the shell to the viewport and move scrolling inside the content column.
- `.app-shell` → `height: 100dvh` (with a `100vh` fallback line first, for older WebView2) + `overflow: hidden`
- `.sidebar` → `height: 100%; min-height: 0; overflow-y: auto` (`auto`, so no nested scrollbar appears
  unless the window is genuinely too short for the nav + footer)
- `.workspace` → `height: 100%; min-height: 0; overflow-y: auto`, dropping `min-height: 100vh`
- the `max-width: 900px` breakpoint (`styles.css:280`) switches the shell to `display: block` with the
  sidebar on top; that mode must be reset to document scrolling or the fixed height would clip it.

`margin-top: auto` on `.sidebar-foot` and `.status-line` is kept — once the boxes are viewport-height it
produces the intended result instead of fighting it.

### 3. Terminal window on launch — missing subsystem attribute

`src-tauri/src/main.rs` is three lines and carries **no** `windows_subsystem` attribute, so the binary links
against the Windows *console* subsystem and the loader allocates a console for it. This is the one real
cause; child processes are already clean:

- `windows_process.rs:177` already passes `CREATE_NO_WINDOW` to `CreateProcessW`
- `lib.rs:1947` already sets `creation_flags(0x08000000)` (`CREATE_NO_WINDOW`)

So ffmpeg/ffprobe/the PowerShell voice worker were never the source of a visible console.

**Fix:** add `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` to `main.rs`. The
`not(debug_assertions)` guard is the point: release bundles get no console, `tauri dev` keeps its stdout, so
the `eprintln!`-based `diagnostic_event` logging and panic output stay visible during development. No
launcher, no extra process, no change to error reporting.

### 4. Voice test panel — the diagnostics have no CSS at all

`App.tsx:791` renders `<dl><dt>Engine</dt><dd>…` and `App.tsx:792` renders `<p role="alert">`. Neither `dl`,
`dt`, `dd` nor that `p` has a single rule in `styles.css` (`h1,h2,h3,p { margin: 0 }` at `:100` does not cover
`dl`). They therefore fall back to UA defaults: **16px font** (the stylesheet never sets a `body` font-size,
so the inherited base is the browser default, while every designed element nearby is 9–11px) and
`dd { margin-left: 40px }`. That is precisely the "disconnected, oversized text" — unstyled UA output
dropped into a 2-column grid (`.voice-test`, `:246`) that auto-places them into the wrong cells.

Secondary issue: results are flattened into one prose string (`App.tsx:739`) joined by `·`, so there is no
way to align labels against values or colour a status.

**Fix:** split the one block into two labelled cards — *Test phrase matching* (text simulation) and
*Test microphone* (live) — and keep the structured `VoiceDecision` in state instead of pre-formatting it into
a sentence, so each field can be rendered as an aligned label/value row with a semantic status pill. Engine
details move into a `<details>` secondary section. No diagnostic field is dropped.

Behaviour is explicitly **not** touched: `testVoice` still calls `test_voice_phrase`, `testMicrophone` still
calls `start_voice_test`/`stop_voice_test` with the same `voiceConfig`, the cleanup effect at `App.tsx:717-720`
is unchanged, and the "no clips saved" guarantee stays a Rust-side property that the UI only reports.

## Implementation plan

1. `main.rs` — one-line conditional `windows_subsystem` attribute.
2. `styles.css` — selection policy; viewport-locked shell + independent content scroll; reset both in the
   `≤900px` breakpoint; new `.voice-lab` card/readout rules reusing existing theme tokens only.
3. `App.tsx` — replace the `.voice-test` block with the two-card `.voice-lab`; add `phraseResult`/`phraseError`
   state in place of the single `voiceResult` string; small local `Readout` row helper.
4. Verify: `cargo fmt --check`, `cargo check`, `cargo test --workspace`, `npm run build` (tsc + vite),
   cross-compiled Windows release build for the subsystem change.

## Deliberately not done

- No new dependency, no CSS framework, no component library.
- No change to capture, FFmpeg lifecycle, library reconciliation, hotkeys, voice matching, or theming.
- No `position: fixed` sidebar or hard-coded offsets — the grid itself is corrected instead.
- No global `user-select: none` on `body` that would break settings inputs.
