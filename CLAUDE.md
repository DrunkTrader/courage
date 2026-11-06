# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Courage is a local-first gameplay instant-replay recorder: a Rust workspace plus a Tauri 2 + React desktop shell. No accounts, cloud services, or network calls. GPLv3.

## Commands

```bash
# Fast loop (run before any PR; see CONTRIBUTING.md)
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
make media-validation          # bash scripts/validate-media-pipeline.sh; requires real ffmpeg/ffprobe on PATH
npm --prefix apps/desktop run build

cargo clippy --workspace --all-targets -- -D warnings
bash scripts/benchmark-core.sh # deterministic core benchmark, 3 runs

# Single test
cargo test -p courage-desktop <test_name>
cargo test -p courage-core <test_name>

# Ignored/real-FFmpeg integration tests (slow, require installed ffmpeg)
cargo test -p courage-desktop real_ffmpeg -- --ignored --nocapture

# Desktop app
npm --prefix apps/desktop install
npm --prefix apps/desktop run desktop        # tauri dev (starts Vite + opens window)
npm --prefix apps/desktop run desktop:build  # tauri build (packaged bundle)

# CLI demo (first reproducible media test — synthetic frames, no display/mic needed)
cargo run -p courage-cli -- demo --output target/courage-demo.mp4 --seconds 4
cargo run -p courage-cli -- trim --input target/courage-demo.mp4 --output target/courage-demo-trimmed.mp4 --start 1 --end 3
```

The desktop crate (`apps/desktop/src-tauri`) needs GTK/glib dev headers to build on Linux (`cargo check`/`test`/`clippy` on it will fail without them — `courage-core`, `courage-store`, and `courage-cli` build fine without those headers and are a reasonable way to verify Rust changes in a minimal environment).

`npm --prefix apps/desktop run build` is `tsc --noEmit && vite build` — it does **not** launch the desktop app; use `run desktop` for that.

## Architecture

**Almost all application logic lives in one file: `apps/desktop/src-tauri/src/lib.rs`** (~2300 lines — commands, settings, FFmpeg command construction, device discovery, library reconciliation, voice session wiring, app/tray setup). Everything else is a focused support module:

- `capture.rs` — `CaptureProcess`/`CaptureState`: owns one recording session's lifecycle (start → recording → stop), its segment directory, and session-directory cleanup with retry-on-failure.
- `process.rs` — `ManagedChild`: the single owner abstraction for *every* child process (ffmpeg, ffprobe, the PowerShell voice worker). Bounded stdout/stderr readers (64 KiB tail, 32 queued lines), graceful-then-forced shutdown with deadlines, idempotent release. New child-process code should go through this, not raw `std::process::Command`.
- `windows_process.rs` (`cfg(windows)`) — raw `CreateProcessW` with an atomic Job Object assignment (`PROC_THREAD_ATTRIBUTE_JOB_LIST`) so a crashed owner can't leak an orphaned ffmpeg/PowerShell process. No shell is ever invoked; arguments go through `CommandLineToArgvW`-compatible quoting.
- `voice.rs` + `voice.ps1` — Windows-only wake/command phrase matching. `voice.ps1` is a `System.Speech` worker (embedded via `include_str!`, run through `powershell.exe`) that emits JSON events over stdout; `Matcher::decide` in Rust is the single, pure, well-tested decision point (confidence threshold, dedup, cooldown, wake-window). Config is sent over stdin as JSON — never interpolated into the script.
- `crates/courage-core` — `trim()` (stream-copy with re-encode fallback) is the only piece the desktop app actually uses. `FrameRingBuffer`/`ClipExporter`/`SyntheticSource`/`VideoSpec` exist only to back `courage-cli`'s synthetic demo/benchmark path — the desktop capture pipeline does not touch them. Don't assume code here is wired into the live app without checking.
- `crates/courage-store` — SQLite (`rusqlite`, bundled) clip metadata: path, duration, thumbnail, favorite. The filesystem is the source of truth for clip *existence*; the store is reconciled against it (`reconcile_library` in `lib.rs`), not the other way around.
- `apps/desktop/src/App.tsx` — the entire React UI (single file, unformatted — no Prettier/ESLint configured for the frontend).

**Capture model:** FFmpeg runs as a long-lived child writing one-second MP4 segments into a per-session `rolling-buffer/session-*` directory (`-f segment`, wrapping at `replay_seconds + 15`, clamped 30–150). Saving a replay picks the newest completed segments by mtime, stream-copies/concats them via a second short-lived FFmpeg into a temp file, then atomically renames into the clips directory. The long-lived capture process is never paused or blocked by a save.

**Concurrency model:** one `Mutex<AppState>` serializes all command work (settings, capture start/stop, library ops); commands run via `spawn_blocking` so the mutex is never held on the async/UI event thread. `Control` (atomics: `stop_epoch`, `exiting`, `operation`, `stops`) implements admission control so a queued start can't resurrect capture after a later stop/exit has already won the lock — see the `Operation`/`StopRequest` guards and their regression tests in `lib.rs`. A single supervisor thread (50ms poll) drives capture-health checks and voice-event polling.

**Shutdown:** window Close and tray Quit both funnel through one idempotent coordinator (`request_exit` in `lib.rs`) that stops voice, stops capture (graceful ffmpeg `q` + stdin close, then forced Job Object termination on deadline), joins pipe readers, and only then lets Tauri exit. Cleanup failure keeps the app open and reports the exact failed path/session for retry — it does not force-close.

**Platform video/audio backends** (not abstracted behind a trait — see `video_input_args`/`audio_input_args`/`system_audio_input_args` in `lib.rs`):
- Windows: `gdigrab` video, DirectShow mic, (claimed) WASAPI system audio.
- macOS: `avfoundation` for both video and audio.
- Linux: `x11grab` video (requires `DISPLAY`; Wayland/PipeWire not implemented), PulseAudio mic + sink monitor for system audio.

## Known-bad areas (don't assume these work — verify before relying on them)

- **System audio + microphone together fails to start capture at all** — the generated `-filter_complex` + `-af` combination is rejected by FFmpeg. See `audit/ENGINEERING_REVIEW_2026-11-06.md` (P0-1) for the reproduced fix (move `aresample` inside the filter graph, drop `-af`).
- **Windows system audio (`-f wasapi`) does not exist in stock FFmpeg builds** — P0-2 in the same review.
- **No CI exists.** The Windows-only tests (Job Object, crash, sharing-violation — see `process.rs`/`capture.rs` `#[cfg(windows)]` tests) have never been run; only compile-checked via cross-compilation.
- The desktop crate's own test/clippy status on non-Windows, non-GTK environments is unverified in this repo's history — see `audit/test_results.md` for what has and hasn't actually been executed on real hardware.

## Audit trail

`audit/` contains a sequence of prior review/implementation-progress documents. They record *what has actually been verified* (e.g., which checks ran on Linux vs. which remain Windows-hardware-only) as distinct from what the code merely claims — read `audit/ENGINEERING_REVIEW_2026-11-06.md` first for the most current, evidence-labeled findings (each marked **Reproduced** / **Code-confirmed** / **Plausible**) before trusting a behavior claim elsewhere in the docs.

## Conventions

- Rust 2021, `rustfmt.toml` pinned (`max_width = 100`). `rust-toolchain.toml` pins stable + rustfmt/clippy components.
- All child-process spawning goes through `ManagedChild` (never bare `Command::spawn` for anything long-running or requiring cleanup guarantees) — the one exception is `courage-core/src/editor.rs` (`trim`), which currently bypasses this (tracked as a known gap, not a pattern to copy).
- No shell interpolation anywhere: args via argv, or JSON over stdin for the PowerShell voice worker.
- Error messages at the Tauri command boundary are deliberately generic/actionable for the UI; detailed diagnostics go to `eprintln!`-based structured JSON events (`diagnostic_event` in `lib.rs`), not the user-facing error string.
