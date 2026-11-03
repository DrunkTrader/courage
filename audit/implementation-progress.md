# Product gap implementation progress

Started: 2026-11-03  
Source: [`PRODUCT_AND_MARKET_GAP_REPORT.md`](PRODUCT_AND_MARKET_GAP_REPORT.md)

## Delivery rule

Implement the smallest local-first slice that is useful and testable. Reuse
Tauri's asset protocol, FFmpeg, SQLite, and existing process ownership. Do not
add accounts, uploads, telemetry, game injection, or a streaming suite.

## Milestones

| ID | Report item | Status | Evidence / next step |
|---|---|---|---|
| A | In-app preview, thumbnails, and trim | **Implemented; native acceptance pending** | Thumbnail generation, scoped assets, `<video>` preview, bounded trim validation, stream-copy attempt, and re-encode fallback are in place. |
| B | System audio plus microphone tracks | **Implemented; host acceptance pending** | Persisted source toggles, Linux Pulse monitor mapping, Windows WASAPI mapping, mixed AAC output, and optional separate microphone track are in place. |
| C | Visual/actionable library | **Implemented** | Real thumbnails, favorites, favorite-only filtering, copy-path, preview, trim, and safe thumbnail cleanup are in place. |
| D | Game/app detection and profiles | Planned | Requires platform-specific foreground-window/process evidence; start with local metadata after A–C. |
| E | Measured performance/recovery | Existing harness | `benchmarks/scripts/measure-windows.ps1` exists; native measurements remain blocked without Windows hardware. |
| F | Sanctioned event adapters | Planned | Add only after a concrete local event source and rate-limit contract are selected. |
| G | Local-first sharing handoffs | **Implemented** | Copy-path and existing OS open/reveal actions remain local and cloud-free. |
| H | Wayland/PipeWire and native capture | Host blocked | Linux PipeWire and macOS native capture need platform-specific implementation and real-host verification. |

## Acceptance checklist

- [x] Saved/imported clips request generated thumbnails when FFmpeg can decode them.
- [x] Clips are previewed inside Courage through the dynamically allowed Tauri asset protocol.
- [x] A selected clip is trimmed to a new file while the original remains.
- [x] Favorites and basic library filtering persist in SQLite.
- [x] A clip path can be copied without leaving the app.
- [x] Audio source settings persist and produce the requested FFmpeg input map.
- [x] Existing lifecycle, media, store, UI smoke, format, check, test, and build gates pass in the reproducible toolchain.
- [ ] Windows audio/capture behavior is verified on a real host before release claims.

## Verification log

| Date | Check | Result |
|---|---|---|
| 2026-11-03 | Baseline reviewed; existing working-tree cleanup preserved | PASS |
| 2026-11-03 | `cargo fmt --all -- --check` | PASS |
| 2026-11-03 | `cargo check --workspace` with Cargo 1.99.1 | PASS |
| 2026-11-03 | `cargo test --workspace` | PASS: 21 passed, 5 ignored |
| 2026-11-03 | ignored FFmpeg 20-cycle desktop save/restart test | PASS: 10 saved clips and generated thumbnails preserved |
| 2026-11-03 | `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| 2026-11-03 | `make media-validation` with Cargo 1.99.1 | PASS: H.264/AAC demo and 2-second trim |
| 2026-11-03 | `npm --prefix apps/desktop run build` | PASS |
| 2026-11-03 | `audit/benchmarks/check-ui.cjs` with Chromium and Playwright | PASS: preview, copy path, favorite filter, settings and keyboard smoke |
| 2026-11-03 | `git diff --check` with isolated HOME | PASS |

## Deferred honestly

Game-event adapters, foreground-game detection, PipeWire/Wayland capture,
ScreenCaptureKit/VideoToolbox, and the Windows resource matrix are not inferred
from Linux tests. They remain explicit follow-ups until their platform APIs and
hardware evidence are available.
