# Test results

## Critical lifecycle follow-up — 2026-11-02, baseline `903fa83`

Environment: Linux aarch64 container, Rust/Cargo 1.99.1; real installed FFmpeg;
Windows GNU target and MinGW for compile checks. No interactive Windows host,
DirectShow device, Windows speech recognizer or WebView2 instance is available.

| Check | Result | Actual evidence / boundary |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | exit 0 |
| `cargo check --workspace` | PASS | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | exit 0 |
| `cargo test --workspace` | PASS | 15 desktop + 3 core + 1 store tests; five desktop tests explicitly ignored by default (three child fixtures, two real-FFmpeg checks); rerun after final queued-start race fix |
| `cargo test -p courage-desktop real_ffmpeg_twenty -- --ignored --nocapture` (run using `real_ffmpeg` filter before adding forced test) | PASS | 20 unique sessions: 10 start/stop, 10 start/save/stop; all removed; ten saved clips probed and retained. Test execution 17.60 s; synthetic 64×64 video and sine audio, no hardware microphone/display. |
| `cargo test -p courage-desktop real_ffmpeg_forced -- --ignored` | PASS | real FFmpeg `-nostdin` ignores graceful request; deadline invokes owned force termination, child reaped/readers joined, 0.12 s test execution |
| Windows-target all-target clippy | PASS | `cargo clippy -p courage-desktop --target x86_64-pc-windows-gnu --all-targets -- -D warnings`; includes compilation of Windows-only job/tree/crash/sharing tests, **not execution** |
| `make media-validation` | PASS | playable H.264/AAC 4.000000 s demo and 2.000000 s trim |
| `npm --prefix apps/desktop run build` | PASS | TypeScript and Vite production assets |
| `node audit/benchmarks/check-ui.cjs` with external Playwright/system Chromium | PASS | Draft values including blank command reach IPC; microphone-test action makes no save call; text autocomplete hints, Ctrl+C/V, Tab, three themes; no page errors. **IPC mocked**, not a native speech/WebView2 test. |
| PowerShell parser checks (`voice.ps1`, `test-windows-lifecycle.ps1`) | PASS | PowerShell 7.6.9 on Linux: syntax only, not Windows PowerShell engine runtime |
| Embedded voice worker on unavailable Linux speech engine | PASS | structured error JSON and exit 1; no successful recognition claim |
| `git diff --check` | PASS | exit 0 |
| Windows release cross-build | PASS | `npm --prefix apps/desktop run desktop:build -- --target x86_64-pc-windows-gnu --no-bundle`; optimized PE32+ x86-64 executable at `target/x86_64-pc-windows-gnu/release/courage-desktop.exe`, 11m02s cold build; final rebuild including queued-start guard passed in 2m44s. Before-build hook rebuilt current frontend. GNU executable imports WebView2Loader.dll, copied alongside from the matching dependency. Compilation/linking evidence, not native runtime/installer acceptance. |
| Native MSVC release / MSI / NSIS installation | NOT RUN | No Windows host; cross-build explicitly used `--no-bundle` |
| `audit/benchmarks/test-windows-lifecycle.ps1` on Windows | NOT RUN | Requires interactive Windows and built executable; no fabricated cycle/PID/privacy result |
| Windows job tree, crash and held-file error 32 tests | NOT RUN | Test source supplied and cross-compiled; controlled sharing-violation reproduction still requires Windows |
| Real microphone accuracy / simultaneous capture / privacy indicator | NOT RUN | Follow `voice-recognition-investigation.md`; no measured improvement or release claim |
| Packaged Saved Info suppression / editable context menus / Chromium 1412 timing | NOT RUN | Configuration/source investigation and browser smoke do not substitute for WebView2 verification |

Default desktop tests cover original-child ownership, graceful acknowledgement, forced
timeout, unrelated-process survival, reaping, reader joining, startup failure/cancellation,
initialization deadline/exit cancellation, repeated/idempotent and concurrent shutdown,
session isolation, cleanup retry with exact blocked path, admission rejection, phrase
normalization, legacy settings and final/confidence/duplicate/expiry/mode/error decisions.

Failures encountered and resolved: libtest's fixture prefix initially hid the fake progress
line (fixture now starts a new line); clippy found a needless borrow (removed); UI smoke's
first Chromium launch failed because the container HOME was unwritable (rerun with writable
HOME passed). PowerShell setup initially selected the wrong architecture and used a newer
Python tar API; corrected to the host's arm64 package/compatible extraction. These are
verification setup failures, not passing Windows runtime results.

### Required Windows evidence still open

- Run native release build/installer; execute job/crash/sharing tests and the lifecycle
  script. Capture exact owner/session/PID/path and privacy observations.
- Verify tray Quit, Close during initialization/export, repeated exit, rapid clicks,
  logoff and forced owner death. Unit cancellation tests do not prove native event delivery.
- Reproduce the original error 32 with exact path/Restart Manager evidence. The original
  external/owned locking process is **not yet identified**.
- Run actual positive/negative speech corpus and packaged autofill/editing checks.

The user's full Definition of Done remains open until those Windows results exist.

### Final self-review

| Axis | Score | Evidence / improvement |
|---|---|---|
| Accuracy | 4/5 | Executed checks and compile evidence are separated from untested Windows behavior; the original error-32 holder is still unknown. |
| Completeness | 2/5 | Hardware/installer portions of the explicit Definition of Done cannot be fulfilled on this Linux host. Run the supplied native matrix before declaring completion. |
| Clarity | 4/5 | Current evidence leads this file; older audit history remains below and must not be read as current acceptance. |
| Actionability | 4/5 | Reproducible scripts and a cross-built executable are supplied; an interactive Windows run is still necessary. |
| Conciseness | 4/5 | Focused runtime changes preserve the UI/features; historical documentation adds some duplication. |

Overall: 3.6/5. Highest-impact follow-up: native ownership/microphone verification,
then the real speech corpus, then installed WebView2/autofill/teardown checks. The user
would reasonably consider the implementation useful but the full Windows task still open.

## Baseline — 2026-11-02

| Command | Result | Evidence |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | clean |
| `cargo check --workspace` | PASS | exit 0 |
| `cargo test --workspace` | PASS | 4 unit tests passed; no failures |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | exit 0 |
| `npm --prefix apps/desktop run build` | PASS | TypeScript + Vite production build |
| Windows Tauri runtime | BLOCKED | no Windows host/hardware in current environment |
| Windows resource benchmark | BLOCKED | no Windows host/result yet |

## Post-change

## UI polish, sidebar layout and Windows startup — 2026-11-06

Environment: Linux aarch64 container, no `DISPLAY`, no GTK/glib dev headers, no Windows host.
Frontend verification used the Vite production build served over localhost and rendered in the
cached Playwright Chromium (`chromium-1243`, aarch64) in headless mode. **This is a Chromium
result, not a WebView2 result**, and it is not a substitute for running the packaged app.

| Check | Result | Actual evidence / boundary |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | exit 0 |
| `cargo check -p courage-core -p courage-store -p courage-cli` | PASS | 15 crates compiled, exit 0 |
| `cargo clippy -p courage-core -p courage-store -p courage-cli --all-targets -- -D warnings` | PASS | exit 0 |
| `cargo test -p courage-core -p courage-store -p courage-cli` | PASS | 5 tests passed, 5 suites |
| `cargo check -p courage-desktop` (host Linux) | **FAIL — pre-existing** | `glib-sys v0.18.1` and `gobject-sys v0.18.0` build scripts fail: `pkg-config --libs --cflags gobject-2.0` exits 1. Missing GTK/glib dev headers, as `CLAUDE.md` documents. Unrelated to this change; no Rust source error was reported. |
| `cargo check -p courage-desktop --target x86_64-pc-windows-gnu --release` | PASS | 247 crates compiled in 3m20s; compiles `main.rs` with the new attribute plus all `cfg(windows)` modules |
| `make media-validation` | PASS | `video=h264 audio=aac duration=4.000000s`, `trim_duration=2.000000s`; unchanged from baseline (no media code touched) |
| `npm --prefix apps/desktop run build` | PASS | `tsc --noEmit` clean; Vite emitted 26.62 kB CSS / 268.80 kB JS |
| PE subsystem flip from the real `main.rs` source | PASS | `main.rs` compiled standalone for `x86_64-pc-windows-gnu` and linked with `x86_64-w64-mingw32-gcc`; PE optional-header subsystem read directly: debug build `3` (`IMAGE_SUBSYSTEM_WINDOWS_CUI`), release build (`-C debug-assertions=off`) `2` (`IMAGE_SUBSYSTEM_WINDOWS_GUI`). Proves the attribute flips the subsystem and that dev builds keep their console. |
| Sidebar fixed during content scroll | PASS | Content column scrolled 800px; `.sidebar` moved 0px and `.sidebar-foot` moved 0px; footer bottom remained inside the viewport |
| Only the intended area scrolls | PASS | Enumerating every element with `scrollHeight > clientHeight` and a scrollable `overflow-y` returned exactly `["workspace-scroll"]`; `documentElement.scrollHeight === clientHeight` (no document scroll, no nested scrollbar) |
| Shell is viewport-height | PASS | `.app-shell` measured height 760 at a 760px viewport and 640 at a 640px viewport; computed `overflow-y: hidden` |
| All nav + `Hide to tray` simultaneously visible | PASS | At 1200x760 and at the configured 960x640 minimum: nav labels `["Library","Capture","Settings"]` all within the viewport, `Hide to tray` button present and within the viewport |
| Non-editable text not selectable | PASS | Computed `user-select`: `h1` = `none`, `.nav-button` = `none` |
| Settings inputs remain selectable/editable | PASS | Computed `user-select`: `#voice-wake` = `text`, `#capture-fps` = `text` |
| Stacked layout below 900px | PASS (by design) | At 880px the viewport lock is released and the document scrolls again (`shellHeight` 3006 > viewport 700). `.sidebar-foot` is hidden there by the pre-existing `≤900px` rule; Tauri `minWidth` is 960, so this mode is unreachable in the app window. |
| Three themes | PASS | Light, Dark and Rusty each rendered the settings view and the new voice panel; semantic status colours resolve from the existing `--good`/`--danger`/`--accent-bright` tokens |
| Voice panel: text simulation distinct from microphone test | PASS | Two separate cards with separate actions and separate error slots; clicking `Test text` outside Tauri produced the `Run the desktop build to test native recognition.` alert in the simulation card only, leaving the microphone card untouched. The microphone button is correctly disabled when `nativeVoiceAvailable` is false. |
| Voice panel: diagnostics readable and aligned | PASS | `Engine`, `Locale`, `Microphone`, `Input format`, `Availability`, `Grammar`, wake/command and last decision all render in aligned monospace label/value rows inside the `Engine diagnostics` `<details>`; no UA-default 16px text remains |
| Packaged NSIS/MSI install and launch on Windows | NOT RUN | No Windows host. This is the only check that can confirm the acceptance criterion "no unwanted terminal window" for the **installed** app end to end. |
| Real microphone / System.Speech recognition in the new panel | NOT RUN | Requires a Windows recognizer; the panel's live values were observed only in their unavailable/stopped state |
| WebView2 rendering of the new layout | NOT RUN | Chromium headless is not WebView2; `100dvh`, `color-mix()` and `user-select` should be verified once on a real packaged window |

Behaviour deliberately unchanged: `test_voice_phrase`, `start_voice_test`/`stop_voice_test`, the
draft-config cleanup effect, and the Rust-side no-save guarantee for microphone tests. The UI only
reports those; it does not enforce them.

| Command | Result | Evidence |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | clean |
| `cargo check --workspace` | PASS | exit 0 with pinned Cargo |
| `cargo test --workspace` | PASS | 3 core + 4 desktop + 1 store unit tests passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | exit 0 |
| `npm --prefix apps/desktop run build` | PASS | TypeScript + Vite production build |
| `npm --prefix apps/desktop run desktop:build -- --debug` | PASS | Tauri debug executable built at `target/debug/courage-desktop` |
| `cargo check --workspace --target x86_64-pc-windows-gnu` | PASS | Windows-target Rust code type-checks with MinGW target installed, including WebView2 setup code |
| `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings` | PASS | Windows-target Rust code passes clippy |
| `make media-validation` | PASS | deterministic demo MP4/audio/trim validation |
| `bash scripts/benchmark-core.sh` | PASS | 3 deterministic core benchmark runs completed; not native Windows capture |
| `git diff --check` | PASS | no whitespace errors |
| Windows capture/speech/hotkey/tray run | BLOCKED | no Windows host or hardware in this environment |
| Windows benchmark script/results | BLOCKED | harness is present; no honest Windows measurements available |

The first desktop-build attempt used the container's older system Cargo and was rejected by the lockfile version; rerunning with the repository's pinned toolchain passed. Hardware-dependent claims remain explicitly unverified until a Windows run is supplied.

Deterministic media evidence: `make media-validation` produced an H.264/AAC
4-second demo and a playable 2-second trim. The core benchmark runs reported
wall times of 5223 ms, 2504 ms, and 1494 ms; these are synthetic Linux pipeline
signals, not Windows capture measurements.
