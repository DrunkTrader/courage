# Courage

> We Don't say “Clip it.” We say **“Courage, have it!”**

Courage is an open-source, local-first gameplay recorder for Windows, Linux,
and macOS. It is designed around a small Rust capture engine and a Tauri 2 +
React desktop shell. There are no required accounts, cloud services, or
network calls.

## Current status

The first vertical slice is implemented. Automated Linux checks and Windows
cross-compilation are recorded in `audit/test_results.md`; Windows hardware and
installer acceptance remain separate from those checks:

- bounded, byte-aware rolling video buffer;
- FFmpeg-backed rolling desktop capture split into bounded one-second segments;
- non-blocking replay save boundary that never pauses the capture process;
- local MP4 export with a user-selected microphone;
- basic local trim/export command that produces a second playable MP4;
- SQLite clip metadata store;
- Tauri 2 desktop shell with a persistent system tray, global save hotkey,
  native notifications, configurable capture settings, local themes, voice
  phrase testing, and a reconciled clip library.

The desktop runtime uses FFmpeg native input backends: Windows `gdigrab` plus
DirectShow microphone devices, macOS `avfoundation`, and Linux X11 plus
PulseAudio. Wayland PipeWire capture and per-GPU hardware encoder selection
still require host verification. On Windows, the wake phrase runs through a
native `System.Speech` listener that continues while the window is hidden in
the tray. System.Speech uses the Windows default input device; the selected
recording microphone is used by FFmpeg for saved clips.

## Quick start

Requirements:

- Rust stable (the project currently uses the stable channel);
- Node.js 20+ and npm;
- FFmpeg with an H.264 encoder for media validation;
- platform-specific Tauri prerequisites for running the desktop shell.

```bash
cargo test --workspace
cargo run -p courage-cli -- demo --output target/courage-demo.mp4 --seconds 4
cargo run -p courage-cli -- trim --input target/courage-demo.mp4 \
  --output target/courage-demo-trimmed.mp4 --start 1 --end 3
ffprobe -v error -show_entries format=duration:stream=codec_name,codec_type \
  -of default=noprint_wrappers=1 target/courage-demo.mp4
npm --prefix apps/desktop install
npm --prefix apps/desktop run build
npm --prefix apps/desktop run desktop
```

### Windows PowerShell

Install these prerequisites first:

- Rust stable with the `x86_64-pc-windows-msvc` toolchain;
- Visual Studio Build Tools with **Desktop development with C++** and a
  Windows 10/11 SDK;
- Node.js 20+;
- WebView2 Runtime;
- FFmpeg and FFprobe on `PATH` for capture, export and media validation;
- Windows 10+ (Job Object process-list attributes) and Windows PowerShell 5.1 with
  a matching installed System.Speech recognition language for voice commands.

From the repository root:

```powershell
npm --prefix apps/desktop install
cargo check --workspace
npm --prefix apps/desktop run desktop
```

`desktop` starts the Vite frontend and opens the Tauri window. The frontend
build alone (`npm --prefix apps/desktop run build`) does not launch the desktop
application. To produce a Windows bundle, use:

```powershell
npm --prefix apps/desktop run desktop:build
```

Windows verification and resource benchmark instructions are in
[`benchmarks/README.md`](benchmarks/README.md). Hardware-dependent
fields remain unreported until a real Windows run supplies the evidence.

For the media validation gate without GNU Make:

```powershell
cargo run --quiet -p courage-cli -- demo `
  --output target/validation/courage-demo.mp4 `
  --seconds 4 --replay-seconds 4
```

The Windows resource icon must exist at
`apps/desktop/src-tauri/icons/icon.ico` before running a Tauri build.

### Recording a real clip

1. Start the desktop app with `npm --prefix apps/desktop run desktop`.
2. Allow display and microphone access when Windows/WebView2 asks.
3. Leave the replay buffer running while you play.
4. Say the configured wake and command phrases (by default **“Courage, have it”**)
   or press the configured global hotkey.
5. Look for the native clip-saved notification, then open Library or the clip folder.

Clips default to the Tauri app-local data directory, normally
`%LOCALAPPDATA%\dev.courage.recorder\clips\` on Windows. Use **Settings → Clip
folder** to choose another directory. Use **Settings → Recording microphone**
to enumerate and select a connected input device; the selected FFmpeg device
is persisted in the local Courage profile.

Closing the main window quits Courage after speech/capture cleanup. **Hide to tray**
explicitly keeps it recording. Tray **Quit Courage** uses the same shutdown procedure.
Cleanup errors keep the app open for retry and report the affected session/path.

Voice Settings select an explicit recognition locale and constrained wake/command
grammar. **Test text** uses the current draft; **Test microphone (no saves)** replaces
the active voice trigger for up to 30 seconds and cannot save clips. Apply restores
the saved trigger. See `audit/voice-recognition-investigation.md` for prerequisites
and the real-microphone accuracy protocol.

PID/session-scoped Windows lifecycle verification:

```powershell
powershell -NoProfile -File .\audit\benchmarks\test-windows-lifecycle.ps1 `
  -Exe .\target\release\courage-desktop.exe -RunBackendTests -ConfirmMicrophoneRelease
```

This exercises 10 start/stop and 10 start/save/stop cycles plus Close while recording.
It records process exit and session removal separately from observed microphone release.

The one-command media gate is:

```bash
make media-validation
```

## Architecture

```text
native capture backends ─┐
                         ├─> CaptureSource ─> bounded rolling buffer
audio backends ──────────┘                         │
                                                  ├─> snapshot job
                                                  └─> FFmpeg exporter ─> MP4
                                                        │
                                                   SQLite metadata
                                                        │
                                               Tauri commands / React UI
```

The core/CLI buffer is bounded by frame count and bytes. The desktop uses a bounded
rolling set of FFmpeg segments; its backend serializes start, export and stop through
one owner while FFmpeg records independently. Blocking commands run off the UI thread.
On Windows, dedicated kill-on-close jobs own capture, speech, probes and exporters;
normal shutdown first requests graceful termination and joins pipe readers before
removing session files. Detailed ownership and bounds are in `audit/CODEBASE_REVIEW.md`.

## Project map

- `crates/courage-core` — media-independent engine primitives and FFmpeg
  process boundary.
- `crates/courage-store` — SQLite schema and clip metadata operations.
- `crates/courage-cli` — deterministic demo/validation executable.
- `apps/desktop` — Tauri 2 desktop shell and React UI.
- `docs/` — architecture, development, and platform backend notes.

## License

GNU GPLv3. See [`LICENSE`](LICENSE).
