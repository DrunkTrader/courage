# Architecture

## Boundaries

`courage-core` owns timing, frame validation, bounded buffering, snapshots,
and encoding jobs. It does not know about windows, Tauri, React, or SQLite.

Native backends will implement source traits and translate platform frames into
the engine's compact `VideoFrame` representation. A backend may use a native
GPU surface internally, but it must hand off with an explicit ownership and
copy policy so the ring's memory ceiling remains measurable.

The capture producer must never wait on FFmpeg or the filesystem. A save
request takes a short lock, clones the bounded requested window, releases the
lock, then encodes on a worker. A production backend should use a bounded job
queue so repeated hotkey presses have backpressure instead of unbounded work.

## Media contract

The first contract is RGB24 frames with a fixed width, height, and nominal
frame rate. FFmpeg converts to `yuv420p` and writes H.264 in an MP4 container.
The contract will evolve to accept hardware-native surfaces only after a
platform benchmark proves that the copy-free path is worth its complexity.

Audio is a separate clocked stream. System audio and microphone input must be
captured independently, timestamped, mixed or preserved according to user
settings, and resampled at the mux boundary. No current code claims that work
is complete.

## Error policy

Errors are typed at library boundaries, carry the failing operation, and are
safe to present in the UI. Permission errors must tell the user which OS
permission is missing. Temporary export files should be atomically renamed only
after FFmpeg exits successfully.
