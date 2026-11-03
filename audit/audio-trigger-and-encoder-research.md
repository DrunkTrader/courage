# Audio trigger and encoder reliability research

**Date:** 2026-11-03  
**Scope:** Courage's Windows voice-trigger path, comparable local/game clip workflows, and FFmpeg encoder selection  
**Decision goal:** improve the existing local-first workflow without adding accounts, cloud speech, telemetry, or a large runtime model

## Executive summary

Courage currently uses Windows `System.Speech.Recognition.SpeechRecognitionEngine`
through a managed PowerShell worker. It is an offline, grammar-based recognizer,
not a wake-word model or a general dictation service. The implementation had two
reliability weaknesses: the grammar was built as a set of string choices and the
Rust matcher required one exact full-phrase result, while the worker used short
silence/babble windows. A natural pause after “Courage” could therefore produce
two valid partial results that Courage deliberately rejected.

The smallest evidence-backed fix is to keep the installed Windows engine, build
an ordered wake→command grammar, accept a bounded two-step utterance, and use
longer recognition windows. This preserves offline operation and avoids shipping
a speech model before a real microphone corpus proves that the Windows engine is
insufficient.

The encoder issue has a clearer root cause: `resolve_video_encoder` silently
returned `libx264` whenever a requested hardware encoder was missing or failed a
one-frame probe. Automatic fallback is useful, but it made an explicit NVIDIA,
AMD, or Intel selection appear to be ignored. Explicit selections now fail with
the probe/build reason; fallback remains available only when both encoder and
hardware preference are automatic.

## Current Courage path

### Audio trigger

- `voice.ps1` starts `SpeechRecognitionEngine` with the installed recognizer for
  the selected locale.
- It reads the default Windows audio device through
  `SetInputToDefaultAudioDevice()`.
- It previously loaded a `Choices` grammar and called synchronous recognition in
  300 ms polling windows.
- Rust normalized punctuation/case, required a final result, rejected low
  confidence, and required the exact combined phrase in one event.
- Courage intentionally does not use the selected FFmpeg recording microphone
  for System.Speech; both device ownership and the actual default device remain
  visible caveats in `audit/voice-recognition-investigation.md`.

### Encoder

- FFmpeg's `-encoders` list is checked first.
- A short `lavfi` probe checks whether the encoder can initialize.
- Before this change, every failed candidate silently became `libx264`.
- On this Linux host, FFmpeg lists `h264_nvenc` but the probe reports
  `Cannot load libcuda.so.1`; that is correct evidence for software fallback in
  automatic mode, not evidence that NVIDIA works on Windows.

## Findings

| Finding | Evidence | Decision |
|---|---|---|
| Constrained vocabularies improve command recognition | Microsoft documents list and SRGS constraints as the mechanism for restricting vocabulary and says restricting active constraints improves accuracy and performance ([S1], [S2]). | Keep a small ordered grammar; do not return to dictation. |
| System.Speech supports timing controls and alternates | Microsoft documents initial silence, babble, end-silence, audio input, `MaxAlternates`, and rejection/recognized events ([S1]). | Increase timing windows modestly and expose readiness/errors; calibrate with real speech later. |
| A one-result exact phrase is brittle | The existing worker emits isolated wake/command grammar results for diagnostics, but Rust rejected both unless the same event contained the full phrase. | Accept full phrase or wake followed by command within four seconds. |
| Medal uses a wake phrase and Windows voice detection | Medal documents “Medal, clip that,” says Voice Clipping uses Windows voice detection, and documents English (US) system-language compatibility ([S3], [S4]). | Keep a distinctive app wake phrase and make locale/default-device requirements explicit. |
| Modern local assistants commonly use VAD + Whisper | The public REX project documents PortAudio capture, Silero VAD, Faster-Whisper, and regex command routing, all locally ([S5]). | Keep as a measured fallback candidate, not an immediate dependency/model download. |
| SteelSeries does not document its voice engine | SteelSeries documents hotkeys, AI/game event auto-clipping, in-app trimming, and hardware encoding, but the cited product page does not identify a speech recognizer ([S6]). | Do not claim a hidden vendor engine; copy the workflow, not an unverified implementation. |
| Windows.Media.SpeechRecognition is not a drop-in replacement | Microsoft documents list/SRGS constraints, but current Windows App SDK guidance requires MSIX package identity for those APIs ([S2]). | Do not switch the unpackaged Tauri runtime without a packaging/API spike. |
| Encoder availability is not encoder usability | FFmpeg documents hardware encoder sections and options ([S7]); local evidence shows a listed NVENC encoder can still fail when CUDA/driver support is unavailable. | Probe the selected encoder and fail explicitly when the user asked for it. |

## Implemented workflow changes

### Voice

1. The worker now builds an ordered grammar by appending wake then command,
   instead of treating the combined phrase as one free-form choice.
2. It loads a small diagnostic grammar for isolated wake/command results.
3. It uses 500 ms initial silence, 4 s babble, 500 ms unambiguous end silence,
   800 ms ambiguous end silence, and 500 ms recognition polling.
4. Rust accepts the exact full phrase as before, or accepts the command after an
   isolated wake result for up to four seconds.
5. The wake window expires, duplicate IDs remain rejected, confidence gating and
   cooldown remain active, and microphone tests still cannot save clips.

### Encoder

1. Automatic mode still tries available hardware encoders and ends at
   `libx264` when no usable hardware encoder exists.
2. Explicit `videoEncoder` or `hardwareAcceleration` choices no longer downgrade
   silently. Capture startup returns whether the encoder is absent from FFmpeg or
   failed initialization.
3. The probe uses two `yuv420p` test frames and a five-second timeout, closer to
   the actual capture format than the old one-frame color probe.

## Recommended next measurement

Run the Windows protocol in `audit/voice-recognition-investigation.md` with:

- the exact phrase 20 times;
- wake then command with pauses of 0.5, 1, 2, and 3 seconds;
- wake alone, command alone, near-sounding phrases, background game audio, and
  simultaneous FFmpeg capture;
- the actual installed locale, Windows default input, selected clip input, and
  reported `AudioFormat`;
- selected `h264_nvenc`, `h264_amf`, and `h264_qsv` one at a time, recording the
  startup result, actual encoder status, save latency, and dropped frames.

If the Windows engine still misses the phrase after this protocol, the next
bounded experiment should be a local VAD + Whisper/sherpa-onnx prototype with a
measured CPU/RAM/latency budget. It should not be merged based on vendor claims
alone.

## Risks and caveats

- No real Windows microphone or GPU was available in this session, so recognition
  accuracy and hardware encoder success remain unverified.
- `System.Speech` still listens to the Windows default input device. The UI must
  continue to say this clearly rather than implying the FFmpeg microphone choice
  controls speech recognition.
- The four-second two-step window reduces false negatives but can increase the
  time during which an isolated wake phrase arms the matcher. Exact command,
  confidence, expiry, and cooldown gates remain required.
- A locally running Whisper/VAD stack would add model packaging, memory, update,
  license, and calibration work. It is a fallback experiment, not a free
  reliability improvement.

## Sources

- **[S1]** Microsoft Learn, [SpeechRecognitionEngine Class](https://learn.microsoft.com/en-us/dotnet/api/system.speech.recognition.speechrecognitionengine?view=netframework-4.8) — installed recognizers, grammar loading, default audio input, timeouts, alternates, and recognition events.
- **[S2]** Microsoft Learn, [Define custom recognition constraints](https://learn.microsoft.com/en-us/windows/apps/develop/input/define-custom-recognition-constraints) — list/SRGS constraints, `CompileConstraintsAsync`, accuracy guidance, and MSIX package-identity requirement.
- **[S3]** Medal Support, [Voice Clipping and System Language Compatibility](https://support.medal.tv/support/solutions/articles/48001265552-voice-clipping-and-system-language-compatibility) — Windows voice detection and English (US) compatibility requirement.
- **[S4]** Medal, [Hands-free CS2 clipping](https://medal.tv/developer/cs2/voice-clip) — wake phrase examples, local-model claim, and background operation positioning. Vendor claims are treated as claims, not independent accuracy evidence.
- **[S5]** David-Antolick, [REX voice assistant](https://github.com/David-Antolick/REX_voice_assistant) — public implementation reference for PortAudio, Silero VAD, Faster-Whisper, and local regex routing.
- **[S6]** SteelSeries, [Moments](https://steelseries.com/gg/moments) — documented hotkey, AI/game-event auto-clipping, trim, multi-track audio, and hardware-encoding workflow; no speech engine identified on the cited page.
- **[S7]** FFmpeg, [Codecs documentation](https://ffmpeg.org/ffmpeg-codecs.html) — encoder availability and hardware encoder documentation.

## Repository evidence

- `apps/desktop/src-tauri/src/voice.ps1`
- `apps/desktop/src-tauri/src/voice.rs`
- `apps/desktop/src-tauri/src/lib.rs`
- `audit/voice-recognition-investigation.md`
- `audit/implementation-progress.md`
