# Windows interactive verification matrix

Run this matrix on a real Windows 10/11 machine with a display, microphone,
WebView2, and FFmpeg. Record the exact build, monitor setup, encoder, and device
names in the generated JSON report. A row is **PASS** only when the observed
behavior and the application state agree.

| ID | Scenario | Pass evidence |
|---|---|---|
| W-01 | Launch from the built executable | Window opens with the Courage icon; no console window; startup timestamp recorded. |
| W-02 | Right-click background, card, input, and select | No browser menu on app content; text selection/editing and native select behavior remain usable. |
| W-03 | Start → stop → start, five cycles | Each cycle creates a new session directory and returns to `capturing`; no sharing violation. |
| W-04 | Start → save → stop → start | Saved MP4 remains playable and the next session starts without touching it. |
| W-05 | Change FPS and resolution, apply | Capture status and FFmpeg behavior reflect the new values after the safe restart. |
| W-06 | Delete a clip in File Explorer | Library removes it on the next status/event refresh without restarting the app. |
| W-07 | Add a stable MP4 externally | Library imports it after reconciliation; a partially copied file is not imported. |
| W-08 | Change output folder and restart | New folder is created/reconciled; existing metadata is not falsely shown as missing. |
| W-09 | Say wake phrase only with default mode | No save occurs. |
| W-10 | Say wake + command, repeat immediately | Exactly one clip is saved; cooldown suppresses the duplicate recognition event. |
| W-11 | Present an intentionally uncertain/low-confidence phrase | Recognition diagnostics show the transient result; no clip is saved below the confidence threshold. |
| W-12 | Switch microphone and test voice | FFmpeg uses the selected recording device; voice status explains that System.Speech uses the Windows default input. |
| W-13 | Record Ctrl/Shift + main `1`, then Numpad `1` | The UI/backend preserve distinct `1` versus `Numpad1` registrations where supported. |
| W-14 | Apply Light, Dark, Rusty, restart | Every screen uses the selected theme with no startup flash after the local preference is set. |
| W-15 | Tray hide, global hotkey, tray save, quit | Hotkey and tray save work while hidden; Quit reaps FFmpeg and exits. |
| W-16 | Run the PowerShell benchmark | JSON/CSV include Windows environment, process samples, forced/graceful stop mode, and no fabricated capture metrics. |

## Required evidence for W-03/W-04

Capture the FFmpeg process list before and after each cycle, the output folder
listing, and the application diagnostic lines for `capture.starting`,
`capture.started`, `capture.stopped`, and any `capture.stop_failed` events.
