# Platform capture roadmap

This document is a boundary and verification checklist, not a claim that the
backends are already implemented.

| Platform | Candidate video API | Candidate audio API | Required verification |
| --- | --- | --- | --- |
| Windows 10/11 | FFmpeg `gdigrab` | FFmpeg WASAPI default capture device | Display permission, microphone device selection, protected content behavior, DWM scaling. |
| Linux | FFmpeg X11 capture; PipeWire portal remains planned | PulseAudio default source | X11 display, multi-monitor, fractional scaling, suspend/resume. |
| macOS | ScreenCaptureKit | Core Audio / AudioUnit | Screen Recording and microphone consent, display changes, sleep/wake. |

Every backend must report: supported sources, pixel format, timestamps,
permissions, hardware encoder candidates, and a recoverable error when no
source is available. CI can compile target modules, but only real OS hosts can
verify capture correctness.

## Current desktop implementation

The Tauri desktop starts FFmpeg as a child process and writes one-second MP4
segments into the app-local `rolling-buffer` directory. The directory is
bounded by FFmpeg's segment wrap count. Saving a replay concatenates only
completed segments, writes an atomic output MP4 into the local `clips`
directory, and inserts its path and duration into SQLite. The capture process
continues while the save operation runs.

The wake phrase is handled by the WebView speech-recognition API. It is a local
trigger boundary from Courage's perspective, but availability and privacy
behavior are controlled by the installed WebView/OS speech service. The UI
reports when the API or microphone permission is unavailable.
