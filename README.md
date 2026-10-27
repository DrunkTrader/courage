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
- deterministic capture source for repeatable development and CI;
- non-blocking snapshot/export boundary;
- FFmpeg MP4 export with H.264 video and optional synthetic AAC audio;
- basic local trim/export command that produces a second playable MP4;
- SQLite clip metadata store;
- Tauri 2 / React / TypeScript desktop shell with the product surface and
  explicit browser-preview state.

Native screen/window/audio capture is intentionally behind interfaces and is
not marked complete until it has been tested on each target operating system.
See the project documentation and development scripts for verification steps
and known limitations.

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
```

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
