# Courage product and market gap report

**Date:** 2026-11-03  
**Scope:** whole repository plus current public competitor surfaces  
**Confidence:** high for repository facts; medium for competitor feature claims because vendor pages are marketing sources

**Baseline note:** the repository facts and gap table describe the pre-implementation
baseline. The delivered changes and current verification state are maintained in
[`implementation-progress.md`](implementation-progress.md).

## Executive summary

Courage has a credible local-first instant-replay foundation: bounded rolling capture, segmented FFmpeg recording, copy-based replay assembly, SQLite metadata, global hotkey capture, Windows voice triggering, cleanup ownership, and explicit platform verification notes. The core recording path is further along than the product workflow around it.

The market is split into three groups:

1. **GPU/OS capture:** NVIDIA App, AMD Adrenalin, Xbox Game Bar, and OBS make capture available or cheap, but they either depend on one hardware vendor, expose a broad/complex workflow, or stop at basic recording.
2. **Clip products:** Medal, Outplayed, and SteelSeries Moments win on automatic clipping, game awareness, timelines, editing, sharing, and mobile/community loops.
3. **Focused open-source tools:** Rewind, Captail, KAPTUR, and Vice show that local-first users still value low overhead, automatic game following, hardware encoding, in-app playback/editing, and privacy-preserving operation.

The highest-value additions are therefore not accounts, cloud uploads, or a larger streaming suite. They are the missing local workflow: **see the clip immediately, trim it, keep its audio useful, organize it by game, and prove that capture is light and reliable on real hardware.**

## 1. Whole-project review

### Current product surface

The repository currently documents and implements:

- local MP4 replay capture with no required account, cloud service, or network call (`README.md:5-8`);
- FFmpeg rolling one-second segments, bounded replay settings, copy-based concatenation, and atomic clip output (`README.md:17-20`, `apps/desktop/src-tauri/src/lib.rs:936-1033`, `apps/desktop/src-tauri/src/lib.rs:1371-1421`);
- global hotkey, system tray, local themes, configurable output directory, microphone selection, and Windows-only System.Speech phrase triggering (`README.md:21-32`);
- SQLite clip records with path, duration, timestamp, and a reserved `thumbnail_path` field (`crates/courage-store/src/lib.rs:15-22`);
- library reconciliation for missing and externally added MP4 files (`apps/desktop/src-tauri/src/lib.rs:702-735`);
- a desktop library that opens clips externally, reveals their folder, deletes them, and shows metadata (`apps/desktop/src/App.tsx:807-812`);
- a CLI-only trim/export path rather than an in-app editor (`crates/courage-cli/src/main.rs:26-43`, `crates/courage-core/src/editor.rs:15-68`);
- capture settings for FPS, resolution, quality, H.264 encoder selection, cursor capture, replay length, and one selected microphone (`apps/desktop/src/App.tsx:698-719`, `apps/desktop/src-tauri/src/lib.rs:981-1088`);
- explicit Windows process ownership, shutdown, cleanup retry, and evidence boundaries (`audit/CODEBASE_REVIEW.md:26-85`, `audit/test_results.md:24-29`).

### Gaps found in the product surface

| Gap | Repository evidence | User consequence |
|---|---|---|
| In-app playback and trim | `ClipInspector` only calls `open_path`; trim is CLI-only | A saved moment leaves Courage immediately for review/editing |
| Real thumbnails and preview metadata | The store accepts `thumbnail_path`, but desktop responses do not expose it and the UI renders a CSS placeholder | The library is a list of filenames rather than a visual memory aid |
| System/game audio plus microphone tracks | `audio_input_args` maps one selected audio input; there is no system-audio or per-track model | Users cannot preserve game audio and voice independently |
| Game/app awareness | `captureSource` validates only `desktop`; output names are timestamped `courage-<time>.mp4` | No automatic game following, per-game folders, or game context |
| Automatic event clipping | Saving is button, hotkey, or Windows voice driven; no event adapter exists | Competitors can save a moment without the user pressing a key |
| Hardware/latency proof | Encoder probing exists, but the repository marks real Windows hardware, microphone, and WebView2 acceptance as unverified | The product cannot yet make a measured low-overhead promise |
| Linux Wayland capture | `docs/platform-capture.md` says PipeWire remains planned; current Linux path requires X11 | A large Linux desktop segment cannot use the intended capture path |
| Sharing workflow | The UI opens a local path or folder; there is no copy-link, export preset, or share handoff | Local clips are saved but not quickly moved into a conversation or post |
| Clip organization | SQLite stores only basic file metadata; no game, tag, favorite, playlist, or marker model | The library degrades as the clip count grows |

The most important distinction is between **missing product value** and **unverified implementation**. The lifecycle work is valuable, but it does not substitute for native hardware acceptance; the repository itself correctly keeps that boundary explicit (`audit/test_results.md:24-29`).

## 2. Competitor research

### Feature matrix

| Product | What it does visibly well | What Courage can learn | Fit with Courage |
|---|---|---|---|
| NVIDIA App / ShadowPlay | Instant Replay, manual recording, Highlights, microphone controls, multi-track audio, storage/location controls, AV1 on supported RTX hardware, and high-resolution capture ([S1]) | Hardware-aware codec choices, separate audio tracks, automatic event capture, and clear storage controls | Adopt the workflow ideas; do not adopt NVIDIA-only assumptions |
| AMD Adrenalin | Instant Replay up to 20 minutes, configurable replay interval, Instant GIF, in-game replay overlay, codec choices, separate microphone track, and push-to-talk ([S2]) | Multiple save formats and a quick in-game review path are useful beyond normal MP4 export | Adopt selectively; keep format count small until compatibility is measured |
| OBS Studio | Replay Buffer, global hotkeys, multiple desktop/microphone inputs, hardware encoders, scenes, sources, and filters ([S3]) | The capture primitive is mature and extensible, but Courage should not recreate OBS's broad streaming surface | Reuse concepts or libraries only where they preserve the focused product |
| SteelSeries Moments | Custom hotkey, AI auto-clipping, trimming, multi-track audio, montage creation, filters, favorites, sharing to YouTube/Discord/Reddit/X, hardware encoding, and disk management ([S4]) | The post-save path is a product, not an afterthought; game events and organization create repeat use | Copy local editing/organization; keep sharing optional and local-first |
| Medal | Background capture, instant links, mobile apps, multi-track audio, configurable clip lengths, auto-clipping, broad game support, and 4K/144 FPS claims ([S5]) | Sharing and mobile create distribution, while multiple clip lengths serve different moments | Treat as a cloud/social competitor, not a privacy model to copy |
| Outplayed | Automatic best-moment capture, match timelines, game support, trim/editor flow, social sharing, overlay, and a large supported-game catalog; the listing also includes ads and a subscription tier ([S6]) | Game-session grouping and match timelines are strong library primitives | Add local match/game grouping without inheriting ads or mandatory client infrastructure |
| Xbox Game Bar / Snipping Tool | Native Windows recording and simple capture without installing a dedicated recorder ([S7]) | Courage must be faster than a built-in shortcut and explain why it is worth installing | Differentiate on replay history, local library, voice, cross-platform intent, and reliability |
| Rewind | Open-source cross-platform instant replay, automatic game/event clipping, game-centric library, per-game settings, in-app playback, hardware encoding, and local controls ([S8]) | The closest open-source product signal is game context plus a visual library | Strong feature reference, but its alpha/untested hardware status is a caution |
| Captail | Focused local replay, continuous recording, automatic desktop/game switching, watchdog recovery, hardware codecs, separate audio tracks, built-in playback/trim, and no cloud/telemetry ([S9]) | Reliability messaging, codec detection, in-app editing, and local-only positioning are directly relevant | High-priority reference for Courage's privacy-first wedge |
| KAPTUR | Hardware capture, no injection, no re-encoding, keyframe-aligned one-second ring segments, and reported low CPU/save latency ([S10]) | Measure and publish capture overhead and save latency; segment design can make save nearly a copy | Use as a benchmark target, not as an unverified promise |
| Vice | Linux replay, GPU-level capture, in-app multi-track editor, playlists, game naming, public Discord links, multiple clip hotkeys, and compositor support ([S11]) | Wayland support, game labels, and clip presets are high-value Linux signals | Adopt Wayland and local organization first; public links remain optional |

Vendor metrics and feature counts above are claims from the linked product pages, not independent benchmarks. The open-source projects are stronger evidence of feature direction than of production readiness; their own pages disclose alpha, preview, hardware, or platform limitations.

## 3. What to add

### Priority 0 — complete the local moment workflow

#### A. In-app preview, thumbnail, and trim

**Add:** generate one thumbnail at save/import time, expose it through `ClipResponse`, show a real `<video>` preview in the inspector, and add a minimal single-range trim action. Keep the existing CLI trim path as the first implementation source.

**Why:** SteelSeries, Outplayed, Captail, Rewind, and Vice all make review/editing part of the core loop. Courage currently saves a clip, displays a placeholder card, and launches an external application.

**Smallest useful version:** one thumbnail, one preview, one trim range, one new output file. Do not build a multi-track timeline yet.

**Acceptance:** save a clip, preview it without leaving Courage, trim it without re-encoding when stream-copy is safe, and retain the original until the new file is confirmed.

#### B. Separate game/system audio from microphone audio

**Add:** a source model with `system`, `microphone`, and optional `mixed` output; default to a compatibility-friendly mixed track plus separate tracks when the container/player supports them.

**Why:** NVIDIA, AMD, Medal, SteelSeries, Captail, and Vice all expose separate or multi-source audio workflows ([S1], [S2], [S4], [S5], [S9], [S11]). Courage currently selects one microphone input and maps it as the only audio input (`lib.rs:1078-1088`).

**Smallest useful version:** Windows system loopback plus selected microphone, with one mixed AAC track and an opt-in separate-mic track. Add Linux/macOS backends after platform measurements.

**Risk:** multi-track files are not equally supported by players and social sites. Keep a mixed compatibility track rather than forcing users to repair audio after export.

#### C. Make the library visual and actionable

**Add:** actual thumbnails, play/trim buttons, copy path, favorite, and a compact filter by date/game/tag once those fields exist.

**Why:** the current SQLite schema already has a thumbnail placeholder, but the UI has no preview asset or organization beyond recent-first ordering. Moments, Outplayed, Rewind, Captail, and Vice all treat the library as a primary product surface ([S4], [S6], [S8], [S9], [S11]).

### Priority 1 — differentiate without breaking local-first

#### D. Game/app detection and per-game profiles

**Add:** detect the foreground application, allow an explicit app/game entry, and store only local metadata. Profiles should override replay length, output folder, audio sources, encoder, and hotkeys.

**Why:** Outplayed, Moments, Rewind, Captail, and Vice use game context for automatic capture, naming, folders, match timelines, or settings ([S4], [S6], [S8], [S9], [S11]). Courage currently rejects every capture source except full desktop and names clips by timestamp.

**Smallest useful version:** process/window detection plus a local executable-to-name map; no network catalog and no game injection.

**Risk:** detection can be wrong, anti-cheat rules vary, and process watchers add lifecycle complexity. Start with naming/folder metadata before automatically switching capture sources.

#### E. Publish measured capture performance and recovery

**Add:** a repeatable benchmark report for CPU, GPU, memory, save latency, dropped frames, disk use, and recovery behavior across NVIDIA, AMD, Intel, X11, Wayland, Windows, and macOS as hardware becomes available.

**Why:** KAPTUR leads with measured CPU/save/memory numbers ([S10]); Captail documents tested versus unverified GPU families ([S9]); Courage currently has encoder probing and synthetic media checks, but native Windows runtime remains explicitly unverified (`audit/test_results.md:24-29`).

**Smallest useful version:** Windows matrix first: H.264 software, NVENC, AMF, QSV; 1080p60; 30/60/120-second replay; save latency and process/resource samples.

#### F. Add opt-in sanctioned event adapters

**Add:** an event adapter boundary that can request a local save with a reason and label. Start with one low-risk source such as local Steam achievement change detection or a documented game-local API; keep manual hotkey mode universal.

**Why:** NVIDIA Highlights, Moments AI, Medal auto-clipping, Outplayed match capture, and Rewind event adapters all reduce the burden of pressing the key ([S1], [S4], [S5], [S6], [S8]).

**Guardrails:** no memory reading, injection, anti-cheat bypass, or broad transcript storage. Every adapter must be opt-in, local, rate-limited, explainable in the library, and removable without affecting manual capture.

### Priority 2 — distribution and platform breadth

#### G. Add local-first sharing handoffs

**Add:** copy path, copy file, open containing folder, and an optional OS share/open-with action before considering hosted links.

**Why:** Medal, Moments, Outplayed, and Vice make sharing immediate ([S4], [S5], [S6], [S11]), but mandatory cloud would violate Courage's current local-only promise.

**Decision:** do not add accounts, uploads, ads, telemetry, or a hosted clip service to the core. If public links become a product requirement, make them a separate opt-in service with a separate architecture decision.

#### H. Finish Wayland/PipeWire and native capture verification

**Add:** PipeWire portal capture on Linux and native ScreenCaptureKit/VideoToolbox path on macOS where FFmpeg's generic path cannot provide the required behavior.

**Why:** the platform roadmap explicitly marks Linux PipeWire as planned and native hardware/permissions as unverified (`docs/platform-capture.md:3-16`). Vice's Linux surface demonstrates demand for Wayland/compositor coverage ([S11]).

**Decision:** treat this as platform completion, not a feature race. Do not advertise cross-platform parity until each backend has real-host evidence.

## 4. What not to copy

- **Cloud-first clip communities:** Medal, Moments, Outplayed, and Vice demonstrate distribution value, but accounts, uploads, ads, subscriptions, and public links add privacy, operating cost, moderation, and support obligations. Keep local export as the default.
- **Broad OBS-style production tooling:** scenes, transitions, plugins, and streaming are useful for OBS users ([S3]) but would erase Courage's one-action replay identity. Integrate or reuse capture primitives instead of becoming another streaming suite.
- **Unbounded AI auto-clipping:** automatic highlights are attractive, but false positives, per-game maintenance, model cost, privacy, and anti-cheat/ToS questions make this a later opt-in layer, not the foundation.
- **Unmeasured hardware promises:** NVIDIA, AMD, and open-source projects expose impressive codec/latency surfaces, but Courage should only expose a codec or claim an overhead after a real-host benchmark.

## 5. Recommended product position

Position Courage as:

> **The quiet, local-first instant replay that lets you see, trim, and keep the moment without an account.**

The defensible wedge is not “more recording settings.” It is the combination of:

- local-only defaults and transparent file ownership;
- strong lifecycle behavior and recoverable cleanup;
- one-action hotkey plus the existing voice trigger;
- a visual, searchable local library;
- measured low-overhead capture across platforms;
- optional sanctioned game events without injection or cloud dependence.

This keeps Courage meaningfully smaller than OBS, less vendor-locked than ShadowPlay/Adrenalin, and less cloud/social-dependent than Medal, Moments, and Outplayed.

## 6. Suggested sequence

1. **Preview loop:** thumbnail → in-app playback → minimal trim → copy path.
2. **Audio loop:** system audio + microphone, mixed compatibility track, optional separate tracks.
3. **Library loop:** game/app label, per-game folder/profile, favorite/filter.
4. **Proof loop:** native Windows benchmark and real-host acceptance; then Linux Wayland/macOS backend work.
5. **Automation loop:** one sanctioned event adapter with visible reason labels and rate limits.
6. **Distribution choice:** revisit optional sharing only after local workflow retention is measured.

## Sources

### Repository sources

- [P1] `README.md` — current product claims, local-first boundary, capture settings, and architecture.
- [P2] `apps/desktop/src/App.tsx` — current library, settings, hotkey, voice, and external-open UI.
- [P3] `apps/desktop/src-tauri/src/lib.rs` — FFmpeg command construction, audio mapping, encoder probing, replay save, and library reconciliation.
- [P4] `crates/courage-store/src/lib.rs` — SQLite clip metadata model, including the unused thumbnail field.
- [P5] `docs/platform-capture.md` and `audit/test_results.md` — platform roadmap and unverified native evidence.

### Public competitor sources

- [S1] [NVIDIA App](https://www.nvidia.com/en-us/software/nvidia-app/) — Instant Replay, Highlights, recording quality, hardware/AV1 capture, and overlay capture claims.
- [S2] [AMD Record settings / Instant Replay](https://www.amd.com/en/resources/support-articles/faqs/DH3-023.html) — replay duration, GIF, codec, microphone, separate track, and in-game replay settings.
- [S3] [OBS Studio Overview Guide](https://obsproject.com/kb/obs-studio-overview) — Replay Buffer, hotkeys, audio devices, hardware encoders, scenes, sources, and filters.
- [S4] [SteelSeries Moments](https://steelseries.com/gg/moments) — auto-clipping, editing, multi-track audio, montage, sharing, gallery, hardware encoding, and disk management.
- [S5] [Medal](https://medal.tv/) — background capture, instant sharing, mobile, multi-track audio, auto-clipping, clip lengths, and supported-resolution claims.
- [S6] [Overwolf Outplayed](https://www.overwolf.com/app/Overwolf-Outplayed) — automatic capture, match timeline, game catalog, editor/sharing flow, overlay, ads, and subscription listing.
- [S7] [Microsoft: How to record your screen on Windows 11](https://www.microsoft.com/en-us/windows/learning-center/how-to-record-screen-windows-11) — native Snipping Tool and Xbox Game Bar recording baseline.
- [S8] [Rewind](https://github.com/abdelaziz-mahdy/rewind) — open-source cross-platform replay, game/event clipping, game library, playback, and hardware-encoding direction.
- [S9] [Captail](https://github.com/FaulMit/captail) — focused local replay, watchdog recovery, hardware codecs, multi-track audio, built-in player/editor, and privacy boundary.
- [S10] [KAPTUR](https://github.com/tappers7/KAPTUR) — hardware/no-injection/no-re-encode positioning and published CPU, memory, and save-latency measurements.
- [S11] [Vice](https://github.com/splitshotz/Vice) — Linux/Wayland replay, GPU capture, editor, playlists, game naming, hotkey presets, and sharing.

## Methodology and caveats

The repository review covered tracked source, configuration, documentation, tests, scripts, benchmark harnesses, and current working-tree product behavior. External research used official vendor/product pages and public project repositories accessed on 2026-11-03. Competitor pages are evidence of visible positioning and feature direction, not independent proof of performance, retention, privacy, or reliability. The implementation that followed this report is tracked in [`implementation-progress.md`](implementation-progress.md); native hardware and platform claims remain unverified until the listed hosts are tested.
