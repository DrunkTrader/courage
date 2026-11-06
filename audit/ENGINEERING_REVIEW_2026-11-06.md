# Courage engineering review

**Date:** 2026-11-06
**Baseline:** `development-branch` at `4053e00`
**Scope:** all tracked source, config, scripts, docs and earlier audit files. No source was changed; this file is the only addition.

## How to read this

Each finding carries an evidence label, so a claim is never stronger than its proof.

| Label | Meaning |
|---|---|
| **Reproduced** | Run here with real FFmpeg using the same arguments the code generates, or checked against upstream source. Experiments used synthetic `lavfi` inputs. |
| **Code-confirmed** | The behaviour follows directly from the code cited. Not executed. |
| **Plausible** | The reasoning is sound, but it needs a Windows or hardware run to confirm. |

### What was run

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | Pass |
| `cargo test` for `courage-core`, `courage-store`, `courage-cli` | Pass: 5 tests |
| `cargo clippy -D warnings` on those three crates | Pass |
| `tsc --noEmit` | Pass |
| `npm audit` | 0 vulnerabilities |
| `cargo test` / `clippy` for `courage-desktop` | **Not run.** This host lacks the GTK/glib dev packages, so the crate does not compile here. The desktop crate holds most of the logic and all 29 of its test attributes. |
| Windows behaviour | **Not run.** No Windows host. |
| `cargo audit` | **Not run.** Not installed. |

### Findings at a glance

| Priority | Count | Theme |
|---|---|---|
| P0 | 4 | Audio settings that stop capture, a privacy bug, metadata loss |
| P1 | 10 | Silent failures, Windows packaging, lock contention, test gaps |
| P2 | 19 | Structure, robustness, smaller bugs |
| P3 | 13 | Dead code and housekeeping |

---

## What is already good (leave it alone)

- **Process ownership** (`process.rs`, `windows_process.rs`). One owner per child, bounded readers, deadlines, and atomic Job Object assignment at creation. This is careful work that avoids a real race. Keep it.
- **Admission control** (`Operation`, `StopRequest`, `stop_epoch` in `lib.rs:234-295`). Small, and it has a regression test for the queued-start race.
- **Cleanup that can retry** (`capture.rs:112-131`). It keeps the owner on failure and reports the exact path. The test proves it will not recursively delete a foreign directory.
- **Voice matcher** (`voice.rs`). A pure, well-tested decision function with a single decision point. Phrases travel over stdin as JSON, never interpolated into PowerShell.
- **The segment design.** 30 one-second segments concatenated into a 30.26 s clip with **zero** audio or video timestamp discontinuities (synthetic input, stream copy). The architecture is sound; do not replace it.
- **Atomic clip save** (temp file, then rename) and clear temp-file exclusion during import.
- **CSP** is strict, with no inline styles or remote content. Capabilities are short.
- **Dependencies** are lean. See [Dependencies](#dependencies).
- **Evidence discipline** in `audit/test_results.md`. Separating "compiled" from "ran on Windows" is the right habit.

---

## P0: fix immediately

### P0-1. System audio plus microphone stops capture from starting

- **Evidence:** Reproduced. FFmpeg exits 234: `Simple and complex filtering cannot be used together for the same stream.`
- **Problem:** `build_capture_command` (`lib.rs:1088-1109`) routes two audio inputs through `-filter_complex`, then always adds `-af aresample=...`. FFmpeg rejects that combination. It fails with and without the separate microphone track.
- **Why it matters:** This is not just an audio failure. `start_capture` fails, so **there is no replay buffer at all**. It affects every platform with system audio, and the feature is marked implemented in `implementation-progress.md` (milestone B).
- **Fix (verified, exit 0):** Put `aresample` inside the graph and drop `-af` in the two-input case:
  - Mixed only: `[1:a:0][2:a:0]amix=inputs=2:duration=longest:dropout_transition=0,aresample=async=1:first_pts=0[mixed]`
  - With separate mic: `[2:a:0]asplit=2[micmix][micraw];[1:a:0][micmix]amix=...,aresample=async=1:first_pts=0[mixed];[micraw]aresample=async=1:first_pts=0[mic]`, then map `[mixed]` and `[mic]`. This produced video plus two AAC tracks.
- **Files:** `apps/desktop/src-tauri/src/lib.rs`
- **Complexity:** Low

### P0-2. Windows system audio uses an FFmpeg input that does not exist

- **Evidence:** Reproduced. `ffmpeg -f wasapi` gives `Unknown input format: 'wasapi'` on 6.1.1. Upstream `libavdevice/alldevices.c` on master has no wasapi demuxer, and `libavdevice/wasapi.c` returns 404. A private FFmpeg fork could differ; stock builds do not have it.
- **Problem:** `system_audio_input_args` (`lib.rs:1247`) passes `-f wasapi -i default`.
- **Why it matters:** On Windows, the primary platform, enabling "Capture system audio" stops the buffer from starting, even with the microphone off. Fixing P0-1 alone will not fix this. Game audio is the most important audio source for a game recorder.
- **Fix:**
  1. **Now (Low):** Check `ffmpeg -devices` for the input. If it is missing, disable the toggle with an honest message and never fail the whole capture because of it.
  2. **Real fix (High):** Capture WASAPI loopback in Rust and feed PCM to FFmpeg through a named pipe. Spike this first, because audio/video timestamp alignment is the real risk.
- **Files:** `lib.rs`, `App.tsx`, `README.md`, `docs/platform-capture.md`
- **Complexity:** Low for the guard, High for the feature

### P0-3. Choosing "No microphone" still records the microphone

- **Evidence:** Code-confirmed.
- **Problem:** The dropdown offers "No microphone" (`App.tsx:768`), which sends `null`. `start_capture_inner` treats `None` as "pick a device" and selects the default or first one (`lib.rs:524-535`). `settings_response` does the same (`lib.rs:771-777`), so after Apply the UI shows a device again.
- **Why it matters:** The app records from a microphone after the user chose none. That is a privacy bug in a product whose pitch is user control. The toggle does work; the dropdown does not.
- **Fix:** Remove the "No microphone" option, because `audioEnabled` already expresses this. If you keep it, make `None` mean no microphone in both functions. Removing it is the smaller change.
- **Files:** `App.tsx`, `lib.rs`
- **Complexity:** Low

### P0-4. An unavailable clip folder erases library metadata

- **Evidence:** Code-confirmed.
- **Problem:** `reconcile_library` deletes any row whose file is not visible (`lib.rs:815-819`) and removes its thumbnail. It runs on every 2-second status poll. `is_file()` also returns false for permission and I/O errors. The folder check (`read_dir`) runs *after* the deletion loop, so it cannot prevent it.
- **Why it matters:** Unplug an external drive or lose a network share, and within about two seconds every row for that folder is gone. Favorites are lost for good, and `created_at` resets to the import time when the drive returns. The clips themselves survive, but the library data does not. The UI also blanks the list (`setClips([])`, `App.tsx:305`).
- **Fix:** Before deleting, confirm the file's parent directory exists and is readable, and skip that clip if not. Treat only a definite not-found as "deleted" (`try_exists`). Add a test that removes a folder and asserts rows survive.
- **Files:** `lib.rs` (`reconcile_library`), `App.tsx`
- **Complexity:** Low

---

## P1: high-value improvements

### P1-1. The capture command has no tests, which is how P0-1 and P0-2 shipped

- **Evidence:** Code-confirmed.
- **Problem:** `build_capture_command` takes `&AppState`, runs encoder probes and returns a 6-tuple, so it cannot be unit-tested. The "real FFmpeg" test (`lib.rs:2231-2268`) **writes its own FFmpeg command** instead of using the production one. `make media-validation` tests the CLI's frame exporter, which the desktop app never uses. Nothing validates the arguments users actually run.
- **Fix:** Extract a pure function such as `capture_args(settings, encoder, os, segments) -> Vec<OsString>`. Add unit tests and an `#[ignore]`d real-FFmpeg matrix that swaps device inputs for `lavfi` and covers each audio mode, odd sizes and each quality preset.
- **Files:** `lib.rs`, `scripts/validate-media-pipeline.sh`
- **Complexity:** Medium

### P1-2. No CI, and the Windows-only tests have never run

- **Evidence:** Code-confirmed. There is no `.github/` directory. `audit/test_results.md` records the job, crash and sharing tests as not run.
- **Why it matters:** The riskiest code in the repo has compile evidence only. These tests need no microphone or display, so "no Windows host" need not block them.
- **Fix:** Add one GitHub Actions workflow with `ubuntu-latest` (fmt, clippy, tests, `tsc`, real-FFmpeg tests) and `windows-latest` (tests including the three Windows-only ones). Hosted runners will not cover capture devices or speech, so those stay manual.
- **Files:** new `.github/workflows/ci.yml`
- **Complexity:** Low to Medium

### P1-3. A release build on Windows shows a console window, and logs have nowhere to go

- **Evidence:** Code-confirmed. `main.rs` has no `windows_subsystem` attribute. On Windows a console window would appear beside the app; not observed here.
- **Problem:** Closing that console would end the app without the shutdown coordinator. All diagnostics go through `eprintln!` (`lib.rs:1933`, `capture.rs:133`), and user messages are deliberately generic, so the useful detail exists only in that console. Adding the attribute alone removes the console **and** the only logs.
- **Why it matters:** Most of this project's history is diagnosing Windows failures remotely. A log file is what makes that possible.
- **Fix:** Do these together: add `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`, write `diagnostic_event` and `record` to a size-capped `logs/courage.log` in the data directory, and add a "Open logs" button. Fix P1-6 at the same time, because raw `Command` calls in `courage-core` will start flashing console windows once the app is a GUI binary.
- **Files:** `main.rs`, `lib.rs`, `capture.rs`, `App.tsx`
- **Complexity:** Low

### P1-4. Failures are invisible while the user is playing

- **Evidence:** Code-confirmed.
- **Problem:** Failure is silent in exactly the normal use case, with the app hidden and a game in focus:
  - FFmpeg dying sets `state.error` but emits nothing (`lib.rs:1034-1048`, `1896-1902`).
  - A failed hotkey or tray save shows only an in-app message.
  - The tray tooltip always says "replay buffer armed" (`lib.rs:2037`), even when stopped.
  - Nothing restarts capture.
  - `poll` only checks process exit, so a stalled FFmpeg still looks healthy. Progress lines are ignored after startup.
- **Why it matters:** The product's promise is that the moment was saved. The worst outcome is discovering after a great play that the buffer died an hour ago.
- **Fix:** On unexpected exit, send a native notification, update the tray tooltip, and auto-restart with backoff. Show save failures as notifications too. Detect stalls by watching `frame=` progress.
- **Files:** `lib.rs`, `capture.rs`
- **Complexity:** Medium

### P1-5. Slow work runs while holding the one state lock

- **Evidence:** Code-confirmed for the mechanism; thumbnail failure Reproduced (a 0.3 s clip exits 0 with no file; a corrupt MP4 exits 183). Real-world latency is not measured.
- **Problem:** `with_state` holds `Mutex<AppState>` across concat (30 s limit), ffprobe (5 s), thumbnails (10 s), device discovery (5 s), trim (no limit) and library scans. The earlier `ponytail:` comment at `lib.rs:198` accepted this pending measurement, but three concrete costs are visible now:
  1. **Thumbnail retry loop.** `reconcile_library` retries any clip without a thumbnail (`lib.rs:820-821`) with no memory of failure. One short or corrupt clip spawns FFmpeg on every 2-second poll, forever. With FFmpeg missing, it tries to spawn for every such clip.
  2. **Disk scans during gameplay.** `engine_status` reconciles the whole library every 2 seconds, even while hidden.
  3. **Missed voice triggers.** The supervisor uses `try_lock`. If the lock is busy for over 2 seconds, recognised phrases age past `RESULT_TTL` and are rejected as expired.
  Also, `list_audio_devices` takes the lock without using the state (`lib.rs:398`), and `setInterval` can stack overlapping calls behind a held lock.
- **Fix, in order:**
  1. Record thumbnail failures so each clip is tried once.
  2. Remove reconciliation from `engine_status`. Run it on `list_clips`, window focus and settings changes.
  3. Drop `with_state` from `list_audio_devices`, and add an in-flight guard to the poll.
  4. Run probe and thumbnail after releasing the lock.

  Do not build an actor system. These four steps should be enough.
- **Files:** `lib.rs`, `App.tsx`, `courage-store`
- **Complexity:** Low for steps 1-3, Medium for step 4

### P1-6. Trim bypasses process ownership

- **Evidence:** Code-confirmed.
- **Problem:** `courage-core/src/editor.rs` calls `Command::output()` directly: no timeout, no cancel, no Job Object. `trim_clip` runs it while holding both the state lock and the `Operation` flag.
- **Why it matters:**
  - A hung trim blocks Stop and Quit indefinitely, because `request_exit` waits on the lock. This is the one child that breaks the lifecycle guarantees.
  - Saves are rejected during a trim.
  - It writes straight to the final path, so a crash leaves a partial file that reconciliation imports as a clip. `reencode` also does not delete its output on failure (`editor.rs:144`).
- **Fix:** Run trim through `ManagedChild` with a cancel flag and a time limit, write to a `.tmp.mp4` and rename. This is easiest after P2-2.
- **Files:** `editor.rs`, `lib.rs`
- **Complexity:** Medium

### P1-7. A second launch starts a second recorder

- **Evidence:** Code-confirmed. There is no single-instance handling.
- **Problem:** A hidden tray app makes this likely: the user cannot see the window and clicks the icon again. Result: two FFmpeg captures, two writers to one SQLite file, each reconciling the other's clips, and a hotkey that only one instance can hold.
- **Fix:** Add `tauri-plugin-single-instance` and call `show_main_window` on a second launch. This is a justified new dependency.
- **Files:** `Cargo.toml`, `lib.rs`
- **Complexity:** Low

### P1-8. The installer does not include FFmpeg

- **Evidence:** Code-confirmed. `tauri.conf.json` has no `externalBin`; all calls use bare `ffmpeg` on `PATH`.
- **Problem:**
  - A normal user installs Courage and it cannot record.
  - The failure is poor: device discovery turns a missing FFmpeg into an empty device list (`lib.rs:1392`).
  - Behaviour depends on whichever FFmpeg build is found, which is how P0-2 stayed hidden.
  - Windows spawn goes through raw `CreateProcessW` with a null application name, which includes the current directory in its search order, unlike Rust's `Command`.
- **Fix:** Ship FFmpeg and FFprobe as Tauri sidecars, resolved to an absolute path. GPL licensing fits, but confirm the build's codec and redistribution terms. Add a startup check that names the problem plainly.
- **Files:** `tauri.conf.json`, `lib.rs`, `editor.rs`, `README.md`
- **Complexity:** Medium

### P1-9. Recoverable settings problems stop the app from opening

- **Evidence:** Code-confirmed. Tauri 2.12.5 panics on setup error (`app.rs:1444`).
- **Problem:** `AppState::new` fails if `create_dir_all(output_dir)` fails (`lib.rs:312`) or `settings.json` does not parse (`lib.rs:1917`). So a clip folder on an unplugged drive means the app will not open, with no message once the console is gone. Without a window, the user cannot fix the folder either.
- **Fix:** Fall back to the default folder and show a warning. On a parse failure, rename the file to `settings.json.corrupt`, load defaults and report it. Pairs with P0-4.
- **Files:** `lib.rs`
- **Complexity:** Low

### P1-10. `gdigrab` is a weak base for a gameplay recorder

- **Evidence:** Plausible. Not measured here. `ddagrab` exists upstream (`libavfilter/vsrc_ddagrab.c`).
- **Problem:** `gdigrab` captures through GDI with CPU-side frames, which is a poor fit for 60-120 FPS game capture and is generally unreliable with exclusive fullscreen. The UI offers 120 FPS regardless.
- **Fix:** Run one bounded experiment before committing: `ddagrab` with hardware encoding against `gdigrab` at 1080p60, comparing CPU, GPU, dropped frames and fullscreen behaviour with the existing `benchmarks/` harness. It only changes the input arguments, so it fits this architecture without a native capture rewrite. Keep `gdigrab` as a fallback. Bundling FFmpeg (P1-8) guarantees availability.
- **Files:** `lib.rs` (`video_input_args`), `benchmarks/`
- **Complexity:** Medium

---

## P2: useful improvements

| ID | Problem | Why it matters | Suggested solution | Files | Complexity | Evidence |
|---|---|---|---|---|---|---|
| P2-1 | `lib.rs` is 2,302 lines mixing commands, settings, FFmpeg, devices, library, voice and app setup | Hard to review or test, and it creates a `lib.rs` ↔ `voice.rs` cycle via `normalize_phrase` | Move code without changing behaviour: `settings.rs`, `ffmpeg.rs`, `devices.rs`, `library.rs`. Move `normalize_phrase` into `voice.rs` | `lib.rs` | Medium | Code-confirmed |
| P2-2 | `courage-core` does not contain the product's engine. Desktop uses only `trim`. `FrameRingBuffer`, `ClipExporter` and `SyntheticSource` serve only the CLI demo | The README diagram and `docs/architecture.md` describe a design that is not what ships, and CI confidence goes to unused code | Delete `buffer.rs`, `export.rs`, `synthetic.rs`. Move `ManagedChild`, FFmpeg args, concat, probe and thumbnail into core. Change the CLI demo to run the real segment and concat path. Rewrite the docs | `crates/*`, `lib.rs`, `process.rs`, docs | Medium | Code-confirmed |
| P2-3 | Settings fields are declared in `SettingsInput`, `SettingsFile`, `SettingsResponse`, the `CaptureValidation` mapping (twice), the TS type, and listed by hand in `applySettings` | Adding one setting means about eight edits, and drift is likely | Use `SettingsFile` as input. Build the response with `#[serde(flatten)]` plus extras. Send `settings: draft` from JS (serde ignores unknown fields). Removes roughly 150 lines | `lib.rs`, `App.tsx` | Low | Code-confirmed |
| P2-4 | Segment selection trusts file mtimes (`lib.rs:1490-1509`) | If Windows reports a stale mtime on the open file, a partial segment could enter the concat and the save would fail | Use FFmpeg's `-segment_list` CSV, which lists completed segments in order. Verified working with wrap, with timestamps | `lib.rs` | Medium | Plausible; alternative Reproduced |
| P2-5 | Crash leftovers in `rolling-buffer/` are never removed, by documented choice | Each session can hold up to 150 segments, so repeated crashes leak disk | On startup, remove `session-*` directories whose PID is not running, in line with the existing no-kill policy | `lib.rs` | Low | Code-confirmed |
| P2-6 | If session cleanup fails, Quit and Start are both blocked (`lib.rs:1985-1992`, `517`) | One file held by antivirus means Courage cannot close or restart; only Task Manager helps | Once the child is reaped, leave leftovers for the P2-5 sweep and report a warning. Files can no longer be written, so this is safe | `capture.rs`, `lib.rs` | Low | Code-confirmed |
| P2-7 | After a microphone test, normal voice triggering does not resume (`lib.rs:1789-1799`, `1876`) | Leaving Settings ends the test and voice stays off with no warning until Apply. Documented, but it is a trap | Call `start_voice_listener` when a test ends if recording and enabled | `lib.rs` | Low | Code-confirmed |
| P2-8 | `open_path` opens any path the WebView sends (`lib.rs:754`) | If the WebView were ever compromised, `explorer.exe <path>` could launch a program | Take a clip `id` and resolve the path in Rust; add a separate command to reveal the folder | `lib.rs`, `App.tsx` | Low | Code-confirmed |
| P2-9 | Permissions are wider than needed. The asset scope covers all of `$APPLOCALDATA/**`, including the database and settings; `dialog:default` grants more than `open` | Least privilege in a local-first app | Scope to `$APPLOCALDATA/clips/**` (user folders are added at runtime). Use `dialog:allow-open` | `tauri.conf.json`, `default.json` | Low | Code-confirmed |
| P2-10 | Hotkey registration lives in the WebView (`App.tsx:366-383`) | Voice was moved to Rust so hidden WebViews could not affect it. The main trigger still depends on JS | Register in Rust via the plugin handler and call the save path directly. Removes the JS plugin and permissions | `lib.rs`, `App.tsx`, `default.json` | Medium | Plausible |
| P2-11 | Odd custom sizes pass validation and the probe (rounded even), then fail at encode | Reproduced at 321×241: exit 187, so the buffer will not start | Reject odd sizes in validation, or round in the filter | `lib.rs:945` | Low | Reproduced |
| P2-12 | "Capture cursor" is ignored on Linux and macOS (`lib.rs:1183-1196`) | The setting silently does nothing there | Pass `-draw_mouse` (x11grab) and `-capture_cursor` (avfoundation), or hide the option | `lib.rs` | Low | Code-confirmed |
| P2-13 | `@DEFAULT_SINK@.monitor` is not a name PulseAudio resolves. It matches exact strings, and a unit test asserts the wrong value. PipeWire behaviour unverified | System audio probably fails on plain PulseAudio | Use `@DEFAULT_MONITOR@`, which both servers define. Update the test | `lib.rs:1249`, `2175` | Low | Upstream source read; not run |
| P2-14 | A poisoned mutex makes the app impossible to quit: `request_exit` fails, resets, and keeps the window open | One panic in a command leaves only Task Manager | Use `lock().unwrap_or_else(PoisonError::into_inner)` on the shutdown path | `lib.rs:1980` | Low | Code-confirmed |
| P2-15 | `concat_segments` says "verify FFmpeg is available" for every `ManagedChild::output` error, including timeout and cancel (`lib.rs:1584-1590`) | Misleads users and support | Match on `ErrorKind`. Also trim the 64 KiB stderr before putting it in UI errors | `lib.rs` | Low | Code-confirmed |
| P2-16 | Store API gaps: `insert_clip` doubles as "update thumbnail"; `last_insert_rowid()` is wrong on the upsert path; lookups scan `list_clips()` (`lib.rs:645`, `702`); no `user_version`; imported clips get import time, not file time | Works today, but brittle, and O(n) per action | Add `get_clip` and `set_thumbnail`, use `RETURNING id`, use `PRAGMA user_version`, take file mtime for imports | `courage-store`, `lib.rs` | Low | Code-confirmed |
| P2-17 | `App.tsx` is 926 lines with 33 lines over 300 characters (longest 2,017). No formatter or linter | The JSX is effectively unreviewable; a one-word change produces a huge diff | Add Prettier and format in one dedicated commit. Then split out `SettingsView`, `LibraryView` and `api.ts` | `apps/desktop/src` | Low to Medium | Code-confirmed |
| P2-18 | `measure-windows.ps1` was moved but keeps `..\..\..`, which from `benchmarks/scripts` resolves to the **parent of the repo**, so results land outside it | The benchmark evidence the project needs goes to the wrong place | Use `..\..` and pick one results folder. `STATUS.md` and the README disagree | `benchmarks/` | Low | Code-confirmed |
| P2-19 | Deleting a clip open in the inspector may fail on Windows if the WebView holds the file | User sees a delete error for an unclear reason | Clear the `<video>` source before calling `delete_clip`, and test on Windows | `App.tsx` | Low | Plausible |

---

## P3: optional

| ID | Item | Files | Complexity |
|---|---|---|---|
| P3-1 | `hide_console_window` does nothing: all five callers go through `ManagedChild`, and Windows `Child::spawn` ignores creation flags and always uses `CREATE_NO_WINDOW`. Delete it | `lib.rs` | Low |
| P3-2 | Dead or no-op code: `Stdio` settings overridden by `ManagedChild::spawn`; trailing `libx264` in explicit candidate lists (explicit mode returns first); `id == "@DEFAULT_SOURCE@"` is never true; `.clamp(5,120)` after validation; `trigger === 'voice'` branch | `lib.rs`, `App.tsx` | Low |
| P3-3 | `-movflags +faststart` does not reach segments (Reproduced: `ftyp free mdat moov`). Remove it from the capture command; it is correct on the final concat | `lib.rs:1153` | Low |
| P3-4 | `video_encoder` and `hardware_acceleration` are two settings for one decision. Merge with a migration | `lib.rs`, `App.tsx` | Low |
| P3-5 | Encoder probing runs on every start, up to about 20 s worst case, inside the lock. Cache per app run | `lib.rs` | Low |
| P3-6 | The trim stream-copy fast path failed the ±0.1 s check in both ranges tested (2.54 s and 3.05 s for a 2 s request), so it fell through to re-encode. The result is correct but the attempt is wasted. Drop it or snap to keyframes | `editor.rs` | Low |
| P3-7 | A manual save emits `clip-saved` and also returns the clip, so the UI handles it twice | `lib.rs`, `App.tsx` | Low |
| P3-8 | `persist_settings` has about 25 lines of backup fallback. Rust's `rename` already replaces atomically on Windows. Simplify, and add a file sync before rename | `lib.rs` | Low |
| P3-9 | Mobile template leftovers: `staticlib`/`cdylib` crate types slow builds; `mobile_entry_point` is unused | `Cargo.toml`, `lib.rs` | Low |
| P3-10 | `gen/schemas/` is tracked (about 4,900 generated lines) with only one file ignored. Ignore all of it | `.gitignore` | Low |
| P3-11 | Add `rust-version`. The lockfile needs a recent Cargo, and an earlier audit hit this. Add `clippy` and `build` targets to the Makefile. Add `graphify-out/` to `.gitignore` | Root files | Low |
| P3-12 | Toasts appear as PowerShell (`POWERSHELL_APP_ID`) and exist only on Windows, though the README says "native notifications". Register an AppUserModelID or use the Tauri notification plugin | `lib.rs`, README | Low |
| P3-13 | Linux/macOS: `open_path` never reaps its child, and an app crash leaves FFmpeg running because only Windows has a job. Reap it and use `PR_SET_PDEATHSIG` on Linux | `lib.rs`, `process.rs` | Low |

---

## Testing review

**Strong:** process lifecycle, voice matcher, store migration.

**Missing, in priority order:**

1. **Production FFmpeg arguments**, per audio mode, size and quality, against real FFmpeg (P1-1).
2. **Windows-only tests in CI** (P1-2).
3. **`reconcile_library`** has no tests. Cover missing folder, missing file, stable-file guard, thumbnail failure, external import (P0-4, P1-5).
4. **`completed_segments`**: empty directory, single file, wrapped names.
5. **Microphone selection**: `None`, stale device, no devices (P0-3).
6. **Settings recovery**: corrupt JSON, missing folder (P1-9).
7. **`quote()`** in `windows_process.rs` is pure and security-relevant, with no tests. Add cases for trailing backslashes and embedded quotes. This runs on any Windows CI.
8. **Device output parsers** (dshow, avfoundation, pactl) are parsing logic with no fixtures. Extract the text-parsing part and test it.
9. **Trim**: only the backwards-range case is tested. Add a real-FFmpeg case and a failure-cleanup case.
10. **Frontend**: `check-ui.cjs` is useful but not in `package.json` or any pipeline. Wire it into CI before adding a new framework.

---

## Developer experience

- **CI:** none (P1-2). This is the largest gap.
- **Setup:** the Quick Start is good, but no doc lists the Linux packages the desktop crate needs. This review hit that gap. Add a line, and say that `npm run desktop` needs FFmpeg.
- **Formatting:** Rust is configured and clean. The frontend has nothing (P2-17).
- **Docs vs reality:** `docs/architecture.md` and the README diagram describe the unused frame engine (P2-2). The README lists macOS, but only NSIS and MSI targets are configured, so there is no packaged Linux or macOS path. Say so plainly.
- **Undocumented config:** `COURAGE_VIDEO_DEVICE` (`lib.rs:1187`) appears in no docs.
- **`audit/` sprawl:** ten-plus documents overlap, some partly superseded. Keep `audit/` as history and move current facts into `docs/`. `audit/benchmarks` and `benchmarks/` should merge (P2-18).
- **Commit style:** messages like "added more features" touch 25 files, which makes regressions hard to bisect. Smaller commits with a conventional prefix would help.

---

## Security and reliability summary

No secrets were found in tracked files. No shell interpolation anywhere: all child processes use argv, or stdin JSON for PowerShell. CSP is strong. There is no network surface in the app.

Main items, all covered above: microphone consent (P0-3), `open_path` (P2-8), scope width (P2-9), executable lookup (P1-8), metadata loss (P0-4), startup failure (P1-9), cannot quit (P2-6, P2-14), unbounded trim (P1-6).

One product decision to make: launching the app starts screen and microphone capture immediately (`App.tsx:339`), and voice defaults to on. That suits a replay tool, but consider a first-run confirmation.

---

## Dependencies

The dependency set is in good shape, and every direct dependency is used. `rusqlite` with `bundled` is the right call for a desktop app. `npm audit` found nothing.

| Item | Note |
|---|---|
| `@tauri-apps/plugin-global-shortcut` (JS) | Removable after P2-10 |
| `tauri-winrt-notification` | Could be replaced by the Tauri notification plugin, which would also add Linux and macOS (P3-12) |
| Vite 8, TypeScript 7, `plugin-react-swc` 4 | New major versions exist. No urgency; upgrade deliberately, separately |
| Additions justified | `tauri-plugin-single-instance` only. A size-capped log writer is a few lines, so no logging crate is needed |
| Rust advisories | Not checked. Add `cargo audit` to CI |

---

## Current Architecture

Courage is a Tauri 2 desktop app with a React UI. Almost all behaviour lives in `apps/desktop/src-tauri`.

1. **Startup.** The UI loads settings and clips, then calls `start_capture`.
2. **Capture.** Rust builds an FFmpeg command and runs it as a `ManagedChild`. FFmpeg writes one-second MP4 segments into a per-session folder, wrapping at `replay + 15` segments. There is no frame handling in Rust.
3. **Save.** A button, hotkey, tray action or voice phrase picks the newest completed segments by mtime, stream-copies them into a temp file, renames it, then adds a thumbnail and a SQLite row. FFmpeg keeps recording throughout.
4. **Voice (Windows only).** A PowerShell `System.Speech` child sends JSON results. A 50 ms supervisor thread polls them, and `Matcher` decides.
5. **State.** One `Mutex<AppState>` serialises everything. Atomics in `Control` handle cancellation so Stop and Quit can interrupt work.
6. **Library.** SQLite stores metadata. The filesystem is reconciled against it on every 2-second status poll.
7. **Crates.** `courage-store` is used directly. From `courage-core`, the desktop uses only `trim`; the rest serves a CLI demo.

---

## Key Problems

1. **Audio is broken where it matters most.** System audio stops capture on Windows, and system plus microphone stops it everywhere.
2. **The tests do not cover what ships.** No test runs the production FFmpeg command, and validation exercises unused code. That is why item 1 reached "implemented".
3. **The app can break its privacy and data promises.** "No microphone" is ignored, and an unplugged drive erases favorites.
4. **It fails silently during play.** A dead buffer gives no notification and no recovery.
5. **It is not yet shippable to normal users.** No bundled FFmpeg, a console window, no log file, no single instance, and it will not open if the clip folder is gone.
6. **The Windows-specific guarantees are unproven,** with no CI to run them.
7. **The structure hides this.** A 2,300-line `lib.rs`, a core crate that is not the core, and docs describing a different design.

The foundations are good: process ownership, cancellation and the segment model all hold up. These are mostly gaps around a sound core, and none calls for a rewrite.

---

## Refactoring Roadmap

Each step ships alone and keeps the app working.

1. **Make FFmpeg testable.** Extract pure `capture_args`, add the real-FFmpeg matrix, and fix P0-1, the P0-2 guard, P2-11, P2-12 and P2-13 there.
2. **Add CI** (P1-2), so everything after this is checked on Windows too.
3. **Fix the remaining P0s.** P0-3 and P0-4, each with a test.
4. **Make it supportable.** P1-3 (subsystem and logs), P1-7, P1-9.
5. **Reduce lock pressure.** P1-5 steps 1-3 first, then step 4.
6. **Move the real pipeline into core** (P2-2) and fix trim (P1-6) as part of the move.
7. **Split `lib.rs`** (P2-1) and collapse settings (P2-3). Pure moves, no behaviour change, separate commits.
8. **Format the frontend** in one commit, then split `App.tsx` (P2-17).

Steps 1-5 are behaviour fixes. Steps 6-8 are structural, and doing them later means they are protected by the tests from steps 1-3.

---

## Feature Roadmap

Existing items in `PRODUCT_AND_MARKET_GAP_REPORT.md` remain valid. This list reorders them around what the code shows.

**Essential**

1. **Working Windows system audio** (P0-2). Without it, game clips have no game sound.
2. **Buffer watchdog:** auto-restart, stall detection, and failure notifications (P1-4).
3. **Bundled FFmpeg with a startup self-check** (P1-8).
4. **`ddagrab` capture path,** subject to the measurement in P1-10.
5. **Clip storage limits:** a free-space check before saving, and optional auto-delete of old non-favorite clips. Nothing bounds the clip folder today.

**Valuable**

6. **Launch at login, start hidden.** A replay buffer only helps if it is already running.
7. **Audible save confirmation.** Toasts are often invisible over fullscreen games.
8. **Include the current second in saves.** The in-progress segment is dropped, so a clip can end up to a second before the trigger. Waiting for it to close fixes this.
9. **Game detection and clip naming** (gap report item D).
10. **Diagnostics export:** one button that bundles logs and versions for bug reports.

**Nice to have**

11. Trim timeline with a scrubber instead of number inputs.
12. Copy clip file to clipboard for pasting into chat apps.
13. A second hotkey for a shorter clip length.

**Not recommended now:** cloud sharing, an AI highlights feature, replacing `System.Speech` before the microphone test protocol has been run, and native capture rewrites before `ddagrab` is measured.

---

## Quick Wins

Each is roughly an hour or less.

1. Move `aresample` into the filter graph (P0-1).
2. Remove the "No microphone" option (P0-3).
3. Guard reconciliation against unavailable folders (P0-4).
4. Stop the thumbnail retry loop (P1-5 step 1).
5. Remove reconciliation from `engine_status` (P1-5 step 2).
6. Drop the lock from `list_audio_devices` (`lib.rs:398`).
7. Add the single-instance plugin (P1-7).
8. Handle a missing clip folder and corrupt settings at startup (P1-9).
9. Reject odd resolutions (P2-11).
10. Use `@DEFAULT_MONITOR@` (P2-13).
11. Send `settings: draft` and delete 24 lines from `applySettings` (P2-3).
12. Fix the benchmark path (P2-18).
13. Delete `hide_console_window` and the no-op flags (P3-1, P3-3).

---

## Long-Term Improvements

- **WASAPI loopback in Rust** feeding FFmpeg (P0-2). This is the largest single item.
- **`ddagrab` and zero-copy hardware encoding** on Windows, with published benchmark numbers.
- **`courage-core` as the true engine,** with the desktop as a thin shell (P2-2).
- **PipeWire and Wayland capture,** plus a real macOS path and installers.
- **Move the voice worker off PowerShell** to a small native helper if enterprise policy or startup time proves a problem. Decide from the microphone protocol results.

---

## Recommended Next Step

**Extract the FFmpeg command into a pure, tested function, and fix P0-1 and the P0-2 guard in the same change** (roadmap step 1).

Why this one:

- It fixes the most damaging user-visible bug: a settings toggle that stops the whole product from recording.
- It closes the cause, not just the symptom. Both audio bugs and the odd-size bug share one root: nothing tests the command that actually runs.
- Everything else gets safer afterwards, including the `ddagrab` experiment, which is only a change of input arguments.
- It is about a day of work, it runs on any machine with FFmpeg, and it needs neither Windows nor new dependencies.

CI should follow immediately, so the same tests guard Windows too.

---

## Limits of this review

- Desktop crate tests and clippy could not run here, so its test status comes from earlier audit records and was not re-verified.
- Nothing was run on Windows or macOS. Windows conclusions come from code reading and upstream FFmpeg source.
- FFmpeg experiments used version 6.1.1 on Linux aarch64 with synthetic inputs. Real devices may behave differently for sync and drift.
- Performance findings describe mechanisms. No latency or CPU figures were measured.
- Bugs marked Code-confirmed were traced by reading and not executed, since the desktop crate does not build here.
