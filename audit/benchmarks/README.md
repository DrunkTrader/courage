# Critical lifecycle verification

## Windows (not executed in this Linux session)

Use a normal interactive Windows 10/11 desktop with FFmpeg/FFprobe, WebView2,
Windows PowerShell 5.1 and the configured System.Speech language installed.
Close any earlier Courage instance. Do not confuse its processes with this run.

From the repository root, using the documented MSVC/Tauri prerequisites:

```powershell
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo test -p courage-desktop real_ffmpeg -- --ignored --nocapture
npm --prefix apps/desktop run desktop:build
powershell -NoProfile -File .\audit\benchmarks\test-windows-lifecycle.ps1 `
  -Exe .\target\release\courage-desktop.exe -ConfirmMicrophoneRelease
```

Repeat with the installed executable. `-RunBackendTests` optionally runs both Rust
test commands inside the script. It uses UI Automation, not a hidden production control
port. If WebView2 does not expose the required InvokePattern, it fails with the missing
button rather than pretending clicks worked. Complete the same flow manually and retain
the logs in that environment. Do not run a second app instance during the test.

The script starts Courage (which starts its buffer), tracks the exact child process handle,
checks parent PID and session command line, observes MP4 activity, then exercises ten
start/stop and ten start/save/stop cycles. It verifies termination and directory removal
before restarting, then closes Courage while recording and checks all tracked processes.
Microphone release is a separate optional operator observation; omitted observations are
explicitly **NOT VERIFIED**. Failure records include known session/PID ownership. Emergency
cleanup targets only the exact Courage process launched by this script; its jobs must
reap descendants. There is no process-name-wide termination.

Evidence defaults to a fresh folder in `%TEMP%`, printed at completion, with JSON and
stderr/stdout logs. Store only redacted result summaries in git. See
[`../../benchmarks/manual-matrix.md`](../../benchmarks/manual-matrix.md) for native exit
races, tray, crashes, editable controls and teardown diagnostic checks, and
[`../voice-recognition-investigation.md`](../voice-recognition-investigation.md) for real
microphone accuracy/contending-capture tests. The existing process-resource sampler lives
under `benchmarks/scripts/`; it does not replace these ownership tests.

## Browser-only frontend smoke

`check-ui.cjs` requires Node, Python 3 and an externally installed Playwright module/browser.
Run `npm --prefix apps/desktop run build` first. Set `COURAGE_PLAYWRIGHT_MODULE` to that
module's path and optionally `COURAGE_CHROMIUM` to an installed Chromium executable, then
run `node audit/benchmarks/check-ui.cjs` from the repo root. Port 1421 must be available.

This checks current draft IPC payloads, the no-save test action, editing/Tab behavior,
autocomplete hints and theme preservation. IPC is mocked. It cannot establish native
autofill suppression, speech accuracy, process cleanup or Windows microphone release.
