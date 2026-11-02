# Courage Windows benchmarks

This suite is Windows-compatible and intentionally reports missing prerequisites
instead of inventing measurements.

## Prerequisites

- Windows 10/11 PowerShell 5.1 or PowerShell 7.
- A built Courage executable or `cargo tauri build --debug` output.
- FFmpeg and FFprobe on `PATH` for media checks.
- Permission to start/stop Courage and inspect its app-local data directory.

## Run

From the repository root:

```powershell
pwsh -NoProfile -File .\audit\benchmarks\scripts\measure-windows.ps1 `
  -Executable .\target\debug\courage-desktop.exe `
  -Iterations 5
```

PowerShell 5.1 can run the script with `powershell -NoProfile -File ...`.
Results are written to `audit/benchmarks/results/` as JSON and CSV. The harness
records process startup, working set, CPU samples, process count, and the
environment. Capture-specific fields are marked `not-run` unless the operator
provides a capture scenario and confirms the resulting clip path.
Because the normal Courage close action hides to the tray, the report explicitly
labels whether the sample exited through a graceful window close or a forced
process-tree stop. The forced mode uses `taskkill /T` so the FFmpeg child is not
left running; do not treat a forced stop as shutdown latency.

Use `manual-matrix.md` for the interactive Windows capture/library/voice/hotkey
verification. Compare two process reports with:

```powershell
pwsh -NoProfile -File .\audit\benchmarks\scripts\compare-windows.ps1 `
  -Baseline .\audit\benchmarks\results\baseline.json `
  -PostFix .\audit\benchmarks\results\post-fix.json `
  -Output .\audit\benchmarks\results\comparison.json
```

## Interpretation

The process metrics are direct Windows process measurements. FFmpeg capture
quality, observed FPS, frame drops, microphone presence, and hotkey/voice
latency require a real interactive Windows run and are never estimated by this
script.
