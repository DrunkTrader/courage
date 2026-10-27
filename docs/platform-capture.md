# Platform capture roadmap

This document is a boundary and verification checklist, not a claim that the
backends are already implemented.

| Platform | Candidate video API | Candidate audio API | Required verification |
| --- | --- | --- | --- |
| Windows 10/11 | Windows Graphics Capture / Desktop Duplication fallback | WASAPI loopback + microphone | GPU/monitor capture, protected content behavior, DWM scaling, hotkey permissions. |
| Linux | PipeWire portal for Wayland; X11 capture where available | PipeWire or PulseAudio monitor + source | portal consent, multi-monitor, fractional scaling, suspend/resume. |
| macOS | ScreenCaptureKit | Core Audio / AudioUnit | Screen Recording and microphone consent, display changes, sleep/wake. |

Every backend must report: supported sources, pixel format, timestamps,
permissions, hardware encoder candidates, and a recoverable error when no
source is available. CI can compile target modules, but only real OS hosts can
verify capture correctness.
