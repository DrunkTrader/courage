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
pwsh -NoProfile -File .\benchmarks\scripts\measure-windows.ps1 `
  -Executable .\target\debug\courage-desktop.exe `
  -Iterations 5
```

PowerShell 5.1 can run the script with `powershell -NoProfile -File ...`.
Results are written to `audit/benchmarks/results/` as JSON and CSV. The harness
records process startup, working set, CPU samples, process count, and the
environment. Capture-specific fields are marked `not-run` unless the operator
provides a capture scenario and confirms the resulting clip path.
Normal Courage Close now coordinates a full shutdown; Hide to tray is explicit.
The sampler still labels forced stops separately; do not treat forced stops as
shutdown latency. For owned-PID, session-directory and microphone verification use
[`../audit/benchmarks/test-windows-lifecycle.ps1`](../audit/benchmarks/test-windows-lifecycle.ps1).

Use `manual-matrix.md` for the interactive Windows capture/library/voice/hotkey
verification. Compare two process reports with:

```powershell
pwsh -NoProfile -File .\benchmarks\scripts\compare-windows.ps1 `
  -Baseline .\audit\benchmarks\results\baseline.json `
  -PostFix .\audit\benchmarks\results\post-fix.json `
  -Output .\audit\benchmarks\results\comparison.json
```

## Interpretation

The process metrics are direct Windows process measurements. FFmpeg capture
quality, observed FPS, frame drops, microphone presence, and hotkey/voice
latency require a real interactive Windows run and are never estimated by this
script.
