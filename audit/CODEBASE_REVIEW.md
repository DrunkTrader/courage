# Courage codebase review

Date: 2026-11-02
Branch: `main`
Initial review baseline: `3714105`; critical follow-up baseline: `903fa83`.

## Critical follow-up: confirmed findings versus open Windows evidence

The historical “fixed by inspection” labels below are implementation descriptions,
not Windows acceptance results. The supplied runtime failures supersede them.

- **Confirmed:** main-window Close prevented closing and hid the window, intentionally
  leaving FFmpeg and speech recording. No Job Object bounded child lifetime on crash.
- **Confirmed:** stop used kill/wait, detached stderr readers, then discarded the session
  even when removal failed. This prevented reliable cleanup retry and diagnostics.
- **Not established:** the exact holder of the reported Windows error-32 path. The old
  log contained neither the file path nor owner PID. Waiting for a child already existed;
  it would be inaccurate to claim merely adding `wait()` proves that incident fixed.
- **Confirmed:** speech used DictationGrammar and implicit recognizer culture. Matching
  tests read persisted settings while the UI displayed a draft. These are pipeline/config
  defects; microphone quality, driver contention, language installation and actual command
  accuracy have not been measured on the reporting machine.
- **Confirmed:** Tauri's general autofill defaults to enabled. Wry maps the supported
  config flag to WebView2 `SetIsGeneralAutofillEnabled`; it is now disabled locally.

### Process ownership and release order

`capture.rs::CaptureProcess` owns session identity/directory/state and a `ManagedChild`.
`process.rs` owns the original child handle/PID, stdin, stdout/stderr readers and deadlines.
`windows_process.rs` calls `CreateProcessW` directly (no FFmpeg shell intermediary), with
`PROC_THREAD_ATTRIBUTE_JOB_LIST` and an explicit inherited-pipe `HANDLE_LIST`. Assignment
is atomic at creation, rather than racing `spawn` against `AssignProcessToJobObject`.
The non-inherited Job Object has `KILL_ON_JOB_CLOSE` and no breakaway flags. Creation
fails closed if job assignment is unsupported or denied. Windows 10+ is required.

Admission flags reject duplicate start/save work and cancel startup/export on stop/exit.
Stop generations also invalidate queued older starts even if Stop finishes before they
acquire the state lock; an explicit regression test covers this ordering.
The state mutex serializes the one active capture owner; heavy commands run on blocking
workers, away from the window event thread. A joined 50-ms supervisor polls the current
session, so hidden WebViews do not control recognition lifetime. Full status snapshots
carry monotonically increasing revisions; speech results carry a worker generation and
are matched only against that worker's configuration/current capture session.

Close, tray Quit and `ExitRequested` use one idempotent coordinator:

1. Block new operations and request cancellation.
2. Stop speech (stdin stop/EOF, dispose recognizer; three-second grace).
3. Mark capture Stopping; send `q\n`, flush/close stdin (two-second grace).
4. Poll real root exit and Windows job process count. At timeout terminate only the
   dedicated job; wait up to three more seconds. Unix fallback owns the direct child.
5. Join both readers after EOF (three-second deadline), close process/job handles.
6. Enumerate and close the directory iterator, remove individual session files, then
   the directory. On failure retain the owner/Error state and exact failed path.
   Windows Restart Manager reports locker PIDs when available; no lock owner is killed.
7. Join the supervisor and allow Tauri's ordinary exit/window teardown. Failed cleanup
   keeps the app open and allows Quit/Stop retry. Hide to tray is explicit.

Audio/display handles belong to the terminated native capture process. Exporters and
probes use the same bounded child primitive; export cancellation reaps before deleting
temporary export files. SQLite stores saved paths outside the session directory; there
are no thumbnail workers, named pipes or filesystem watchers touching session files.
Saved clips are never removed by session cleanup. Hard kills/logoff may prevent directory
cleanup; job lifetime still bounds Windows children. Crash leftovers are retained, not
blindly deleted on next launch. No global `ffmpeg.exe` termination exists.

Installed Tao 0.37.2 does not handle `WM_QUERYENDSESSION`; its `WM_ENDSESSION`
handler calls `loop_destroyed()` and then `std::process::exit(0)`. Logoff/shutdown
therefore cannot be claimed to use the normal ExitRequested coordinator. The
non-inherited kill-on-close jobs are the safeguard there; temporary-session deletion
is best-effort and may not execute. This path needs an actual Windows logoff test.

### Bounds and blocking behavior

Each child has two reader threads: at most 64 KiB retained output per stream, 32 queued
stdout lines sourced from at most 8 KiB each, and a 4-KiB read buffer per thread. Payload
storage is bounded below approximately 1 MiB even with worst-case lossy UTF-8 expansion;
allocator overhead and thread stacks are separate. Control writes total at most 2 KiB.
There is one capture child and one speech child, plus a serialized short-lived probe or
export child. FFmpeg/speech engine memory is external and still needs Windows measurement.
Capture readiness waits for a positive progress frame count, at most ten seconds.
Probes have 3–5-second deadlines and exports 30 seconds; cancellation interrupts their
wait. Filesystem/driver system calls can still block in the OS. Serialization is deliberate;
the UI thread must not wait on the state mutex. Linux real-FFmpeg 20-cycle run took 17.60 s
including ten small remuxes; this is correctness evidence, not a gameplay benchmark.

### Independent Chromium warning

Chromium's `ui/gfx/win/window_impl.cc`, `ClassRegistrar::UnregisterClasses`, logs this exact
message when `UnregisterClass` fails; `WindowImpl::UnregisterClassesAtExit` registers it
for teardown. Windows 1412 is `ERROR_CLASS_HAS_WINDOWS` (“Class still has open windows”).
This is distinct from file sharing violation 32. Courage declares one main WebView and
does not explicitly destroy/recreate it. No duplicate-destroy call was found. Frontend
polling/new work now stops on exit admission; Tauri retains ownership of native teardown.
Whether this particular runtime warning occurs only at teardown, persists after orderly
shutdown, or is harmless cannot be established without the affected WebView2 version and
a Windows reproduction. No native class/window hack or diagnostic suppression was added.

Sources: [Chromium source](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/ui/gfx/win/window_impl.cc),
[Windows error 1412](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--1300-1699-),
[Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).
Voice details and microphone test protocol: [voice-recognition-investigation.md](voice-recognition-investigation.md).

## Verified baseline

- Rust workspace: `courage-core`, `courage-store`, `courage-cli`, and the Tauri desktop crate.
- Desktop stack: Tauri 2.12.x resolved by `Cargo.lock`, React 19, TypeScript, Vite 7.
- Native capture: FFmpeg child process; Windows `gdigrab` video and DirectShow microphone enumeration/capture.
- Persistence: SQLite clip metadata plus JSON settings under Tauri app-local data.
- Desktop integrations: Tauri tray icon, global-shortcut plugin, dialog plugin, Windows-only WinRT toast dependency.
- Baseline `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `npm --prefix apps/desktop run build`: passed.
- Windows hardware/runtime: not available in this Linux environment; marked unverified below.

## Historical findings (prior implementation, not current acceptance)

| ID | Severity | Status | Finding and root cause | Affected code |
|---|---|---|---|---|
| C-01 | High | Fixed by code inspection; runtime verification required | The frontend suppresses the browser context menu outside editable controls and browser refresh/print/save/inspect shortcuts; Windows setup also disables WebView2 default context menus and production devtools. | `apps/desktop/src/App.tsx`, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src-tauri/tauri.conf.json` |
| C-02 | High | Fixed by code inspection; runtime verification required | Persisted FPS, resolution, quality, encoder, cursor, and audio settings validate and feed the FFmpeg command. Lower resolutions are output-scaled rather than cropped, and hardware encoder probes fall back to software when unavailable. | `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src/App.tsx` |
| C-03 | High | Fixed by code inspection; Windows runtime verification required | Each capture receives a unique session directory. Start failures remove it, stop kills/reaps the child, drains stderr, and removes the session after process termination. | `apps/desktop/src-tauri/src/lib.rs` |
| C-04 | High | Fixed by code inspection; Windows runtime verification required | Stop and dead-process paths now reap children and report cleanup failures; detached stderr is drained instead of being left unread. | `apps/desktop/src-tauri/src/lib.rs` |
| C-05 | High | Fixed by code inspection; runtime verification required | The backend reconciles SQLite against the output folder, removes missing rows, imports external MP4s, emits library changes, and supports application deletion. | `crates/courage-store/src/lib.rs`, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src/App.tsx` |
| C-06 | High | Fixed by code inspection; Windows speech runtime verification required | Wake and command phrases are persisted, normalized, tested, matched as a combined phrase by default, and guarded by an explicit wake-only mode, confidence threshold, transient recognition diagnostics, and a save cooldown. System.Speech still uses the Windows default input device. | `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src/App.tsx` |
| C-07 | Medium | Fixed by code inspection; runtime verification required | The UI records supported modifier/key combinations and preserves `Numpad*` physical identities supported by the installed global-hotkey parser. Registration is preflighted and failures are surfaced. | `apps/desktop/src/App.tsx`, Tauri global shortcut plugin |
| C-08 | Medium | Fixed by code inspection; restart/runtime verification required | Light, Dark, and Rusty are shared-token themes with immediate application and local persistence. | `apps/desktop/src/App.tsx`, `apps/desktop/src/styles.css` |
| C-09 | Medium | Fixed by code inspection | Controls use a small inline SVG icon set with accessible labels instead of emoji or arbitrary glyphs. | `apps/desktop/src/App.tsx` |
| C-10 | Medium | Fixed by code inspection; Windows package verification required | Existing ICO/PNG assets are referenced by the Tauri bundle, the tray uses the default Courage window icon, and production devtools are disabled. | `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src-tauri/icons/` |
| C-11 | Medium | Fixed by code inspection; runtime verification required | `engine_status` reconciles the library and emits changes; the frontend listens for those events and retains a bounded two-second health poll for recovery. | `apps/desktop/src/App.tsx`, `apps/desktop/src-tauri/src/lib.rs` |
| C-12 | Low | Fixed by code inspection; runtime verification required | Main UI copy prefixes failures with actionable context; structured JSON diagnostics retain FFmpeg/resource details without making raw process errors the primary message. | `apps/desktop/src/App.tsx`, `apps/desktop/src-tauri/src/lib.rs` |
| C-13 | Medium | Blocked | Native Windows capture, speech, tray, hotkey, and file-lock behavior cannot be verified on this Linux host. | Windows runtime/manual matrix |
| C-14 | Medium | Blocked | The Windows harness and structured result directory now exist, but no Windows baseline/post-fix measurement can be produced on this Linux host. | `audit/benchmarks/` |

## Architecture risks

1. The Tauri command state is a single `Mutex<AppState>` and commands perform process startup, device discovery, filesystem work, and remuxing while holding it. This is simple but must not be allowed to block lifecycle commands indefinitely.
2. Unique session directories and managed cleanup now prevent cross-session reuse. The reported Windows sharing violation still needs the new exact-path/locker evidence before its original holder can be confirmed.
3. The store is authoritative only for metadata, while the filesystem is authoritative for clip existence. Reconciliation must be centralized at the backend boundary.
4. The Windows PowerShell recognizer is an external worker and must use persisted phrase configuration, selected input where possible, explicit trigger states, and a cooldown.

## Design contract

- Tone: compact, warm technical utility; daily-use scanning over marketing spectacle.
- Primary task: know whether the buffer is armed and save a replay with one action.
- Secondary tasks: inspect/delete clips and configure capture without exposing inert controls.
- Themes: exactly `light`, `dark`, and `rusty`, all driven by shared CSS variables.
- Content states: loading, empty, populated, missing-file/reconciled, error, and permission/unavailable.
