# Windows result status

No Windows runtime result is checked in yet.

The benchmark harness is implemented and the Linux-verifiable deterministic
media benchmark passed, but this environment cannot produce honest Windows
startup, CPU, memory, FFmpeg device, WebView2, tray, speech, or hotkey numbers.
Run `scripts/measure-windows.ps1` on Windows and keep the generated JSON/CSV
alongside this file for baseline and post-fix comparison.
