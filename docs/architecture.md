# Architecture

## Boundaries

`courage-core` owns timing, frame validation, bounded buffering, snapshots,
and encoding jobs. It does not know about windows, Tauri, React, or SQLite.

Native backends will translate platform frames into the engine's compact
`VideoFrame` representation. A backend may use a native
GPU surface internally, but it must hand off with an explicit ownership and
copy policy so the ring's memory ceiling remains measurable.

The capture producer must never wait on FFmpeg or the filesystem. A save
request takes a short lock, clones the bounded requested window, releases the
lock, then encodes outside the capture path. Add a bounded job queue only when
repeated saves need backpressure.

## Media contract

The first contract is RGB24 frames with a fixed width, height, and nominal
frame rate. FFmpeg converts to `yuv420p` and writes H.264 in an MP4 container.
The contract will evolve to accept hardware-native surfaces only after a
platform benchmark proves that the copy-free path is worth its complexity.

The desktop FFmpeg path can capture a selected microphone and, where the
platform backend is available, the system playback monitor independently. It
mixes both into a compatibility AAC track and can preserve the microphone as a
second track. Core/native backends still need explicit timestamp and hardware
verification on each target host.

## Error policy

Errors are typed at library boundaries, carry the failing operation, and are
safe to present in the UI. Permission errors must tell the user which OS
permission is missing. Temporary export files should be atomically renamed only
after FFmpeg exits successfully.
