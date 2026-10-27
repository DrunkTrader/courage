# Platform capture roadmap

This document is a boundary and verification checklist. The first FFmpeg
desktop path is implemented; real hardware and permissions still need to be
verified on each target OS.

| Platform | Candidate video API | Candidate audio API | Required verification |
| --- | --- | --- | --- |
| Windows 10/11 | FFmpeg `gdigrab` | FFmpeg DirectShow selected input | Display permission, microphone device enumeration, protected content behavior, DWM scaling. |
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
directory (or the user-selected clip directory), and inserts its path and
duration into SQLite. The capture process continues while the save operation
runs.

On Windows, the wake phrase is handled by a native `System.Speech` child
listener. It remains alive after the main window is hidden in the tray and
emits a native notification after saving. The UI reports when Windows speech
or microphone privacy permissions prevent the listener from starting. Other
platforms keep voice trigger disabled until a native speech backend is added.
