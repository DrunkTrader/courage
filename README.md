# Courage

> We Don't say “Clip it.” We say **“Courage, have it!”**

Courage is an open-source, local-first gameplay recorder for Windows, Linux,
and macOS. It is designed around a small Rust capture engine and a Tauri 2 +
React desktop shell. There are no required accounts, cloud services, or
network calls.

## Current status

The repository is being built in dependency order. The first vertical slice is
implemented and verified:

- bounded, byte-aware rolling video buffer;
- FFmpeg-backed rolling desktop capture split into bounded one-second segments;
- non-blocking replay save boundary that never pauses the capture process;
- local MP4 export with microphone audio through the platform's default audio device;
- basic local trim/export command that produces a second playable MP4;
- SQLite clip metadata store;
- Tauri 2 / React / TypeScript desktop shell with the product surface and
  explicit browser-preview state.

The desktop runtime now uses FFmpeg native input backends: Windows `gdigrab`
plus WASAPI, macOS `avfoundation`, and Linux X11 plus PulseAudio. Wayland
PipeWire capture and per-GPU hardware encoder selection still require host
verification. The voice phrase uses the WebView speech-recognition API and
requires microphone permission.

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
- FFmpeg on `PATH` for the CLI/media validation.

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
4. Say **“Courage, have it”** or press **Save last 30s**.
5. Open the Library and click the saved clip to play it.

Clips and the SQLite library are stored under the Tauri app-local data
directory, not in the repository and not as demo files. On Windows this is
normally `%LOCALAPPDATA%\dev.courage.recorder\`.

If the default microphone is not the device you want, set
`COURAGE_AUDIO_DEVICE` before launching the app. FFmpeg's WASAPI device name
must be used. The status line will report a capture startup error instead of
claiming that recording is active when FFmpeg exits.

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

Capture stays on its own producer path. Saving a clip copies a bounded
snapshot and hands encoding to a worker; disk and FFmpeg work never holds the
capture lock. The buffer is bounded both by frame count and bytes so a bad
source cannot grow memory without limit.

## Project map

- `crates/courage-core` — media-independent engine primitives and FFmpeg
  process boundary.
- `crates/courage-store` — SQLite schema and clip metadata operations.
- `crates/courage-cli` — deterministic demo/validation executable.
- `apps/desktop` — Tauri 2 desktop shell and React UI.
- `docs/` — architecture, development, and platform backend notes.

## License

GNU GPLv3. See [`LICENSE`](LICENSE).
