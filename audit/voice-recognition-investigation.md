# Voice recognition and microphone investigation

Status: implementation and deterministic checks complete; actual Windows microphone
accuracy, contention and privacy-indicator release **not verified** in this Linux environment.

## Evidence and confirmed defects

The reported “hunted the bologna may” (24%) and “handed him” (4%) came from an unrestricted
`System.Speech.Recognition.DictationGrammar`, not a command grammar. The recognizer was
constructed without an explicit locale and used the Windows default audio device. Neither
the user's installed recognition language nor the actual selected default device is known.
The logs do not prove bad acoustics or exclusive-device contention. Their low confidence
already made them unsuitable triggers; lowering the threshold would not fix recognition.

The text test used persisted phrases, while its visible controls were unsaved drafts. That
explains how a blank visible command could still report a match. Tests now receive the
draft explicitly; empty combined-mode commands fail validation and never match. Wake-only
mode permits an unused empty command and reports commandMatched=false. Save updates the
actual worker grammar; failed persistence restores previous in-memory settings.

## Engine decision

| Candidate | Suitability, packaging, language and license | CPU/RAM/latency/accuracy evidence |
|---|---|---|
| System.Speech constrained grammar (selected) | Offline desktop .NET Framework API; customizable `Choices` + `GrammarBuilder.Culture`. Uses existing Windows PowerShell 5.1 and installed matching speech language, no model distributed. Subject to Windows/.NET availability and enterprise PowerShell policy. | No local Windows measurements. Restricted vocabulary addresses the confirmed dictation mismatch; does not prove target-command accuracy or eliminate acoustic false positives. PowerShell adds process/startup overhead. |
| Windows.Media.SpeechRecognition command/list constraints | Native Windows alternative; requires separately validating WinRT desktop activation, permissions, supported language and package identity requirements for this Tauri distribution. | Not prototyped or benchmarked; changing APIs before testing the existing constrained engine is not evidence-backed. |
| Vosk small model | Offline, many languages, dynamic vocabulary for most small models; ships native library and model per language. US English small model listed as 40 MB, Apache-2.0; model licenses vary. | Upstream describes typical small-model RAM near 300 MB. These are upstream figures, not Courage measurements. Command accuracy/latency still need the same microphone corpus. |
| sherpa-onnx | Offline Windows libraries, Rust/C interfaces and keyword spotting; requires ONNX runtime/model packaging and per-model licensing review (engine Apache-2.0). | Model-dependent footprint, CPU and latency; no Courage measurements, so no unsupported numeric comparison. |

Selected the smallest change supported by evidence: retain System.Speech and replace
dictation with a constrained grammar and explicit locale. Vosk/sherpa remain candidates
only if actual microphone tests demonstrate the native engine inadequate.

References: [System.Speech API](https://learn.microsoft.com/en-us/dotnet/api/system.speech.recognition.speechrecognitionengine?view=netframework-4.8),
[Vosk model sizes/licenses](https://alphacephei.com/vosk/models),
[sherpa-onnx](https://k2-fsa.github.io/sherpa/onnx/index.html).

## Implemented recognition path

- The embedded `voice.ps1` runs via **powershell.exe**, not PowerShell 7. No script file,
  model download or developer checkout is required at runtime. Windows PowerShell,
  System.Speech, installed locale and microphone permission remain prerequisites of the
  distributed app. The packaged Windows executable still needs runtime validation.
- JSON configuration is sent over stdin; phrases are not interpolated into executable
  PowerShell. Culture selection fails explicitly and lists installed recognizer locales
  rather than silently using another language. Legacy settings default to `en-US`.
- Combined grammar contains an ordered full phrase and isolated wake/command alternatives
  for diagnostics. Exact normalized **full final** utterances trigger, and a command may
  follow an isolated wake result within a bounded four-second window. Wake-only grammar
  contains the wake phrase.
- The Rust matcher is the single decision point. It rejects hypotheses, missing/non-finite
  or below-0.80 confidence, incorrect/partial phrases, duplicate/out-of-order result IDs,
  results older than two seconds, and commands within the two-second cooldown. Confidence
  is an engine score, not a measured probability of correctness. The threshold is not
  yet calibrated against this user's hardware/accent.
- A session-bound worker generation and serialized supervisor prevent old recognition
  results from saving under a new capture/configuration. Test mode is a backend flag that
  cannot call save, even if a phrase matches. It replaces the trigger for at most 30 seconds;
  stopping/leaving/changing the draft ends the test. Apply resumes saved voice settings.
- The UI reports engine, locale, default speech input, input format when provided,
  availability/readiness, last final transcript/confidence, wake/command match, trigger
  reason and whether a clip was saved. It distinguishes text simulation from microphone
  testing. Transcripts remain in bounded transient memory, not on disk.

## Device ownership and release

FFmpeg uses the selected DirectShow device; System.Speech uses
`SetInputToDefaultAudioDevice()`. These may be the **same physical microphone** or different
devices. DirectShow enumeration order is no longer labeled the Windows default. The UI
explicitly names the speech selection as Windows default; its friendly hardware name is
not resolved, and independent speech-device selection is not currently supported.

Neither Courage command requests exclusive audio mode. Driver behavior is still unknown.
FFmpeg resamples output to the selected 44.1/48 kHz and channel count with asynchronous
resampling for A/V sync; those output settings do not set System.Speech's input format.
Speech reports its actual negotiated AudioFormat when available. Capture stop ends both
consumers. The PowerShell worker's finally block calls SetInputToNull and Dispose; a bounded
shutdown/owned Job Object fallback guarantees no deliberately abandoned speech process.
Theme/hotkey-only settings do not reopen speech. Voice changes restart speech alone.

A shared PCM fan-out would need capture ownership, format conversion, timestamps and
backpressure across both consumers. No observed driver failure yet justifies that rewrite.
Compare capture-off/on tests below first. Process exit evidence alone is not labeled a
measured privacy-indicator or driver-release result.

## Real microphone protocol — NOT RUN

Record Windows version, WebView2 version, FFmpeg version, microphone/driver, OS default
input, selected clip input, installed recognizer culture and input format. Use an installed
release build with networking disconnected after prerequisites are installed.

1. Stop capture. Open Settings; select the installed recognition locale and both phrases.
   Run **Test microphone (no saves)**. Say the exact phrase 20 times, then 20 negative
   examples (wake alone, command alone, unrelated speech, near-sounding words). Include
   silence/background playback. Repeat tests as needed because each expires after 30 s.
2. Record accepted/rejected counts, duplicate results and end-of-speech-to-result latency;
   do not infer accuracy from the confidence percentage. Confirm library count unchanged.
3. Repeat with capture recording audio from the **same** device, then a different device
   if available. Compare accuracy/latency and inspect a saved clip for A/V sync.
4. Test wake-only with empty command; combined mode with empty command must be rejected.
   Change both draft phrases, verify text and microphone tests use that draft, then Apply
   and verify only the new saved grammar triggers. Disconnect microphone/remove permission;
   require a visible error and no save, then recover by testing/applying again.
5. Stop test and buffer; verify worker PIDs exit and Windows privacy indicator releases.
   Repeat on Close, tray Quit and a forced owner termination. Keep forced-crash temporary
   files as diagnostic evidence; normal stops must remove session directories.

No positive/negative microphone counts or accuracy improvement is claimed until this
protocol is executed. Unit tests exercise decision semantics and the error protocol only.
