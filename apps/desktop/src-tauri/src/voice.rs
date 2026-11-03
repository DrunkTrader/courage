use crate::normalize_phrase;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

pub const CONFIDENCE_THRESHOLD: f32 = 0.80;
const RESULT_TTL: Duration = Duration::from_secs(2);
const COOLDOWN: Duration = Duration::from_secs(2);
const WAKE_WINDOW: Duration = Duration::from_secs(4);

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceConfig {
    pub wake_phrase: String,
    pub command_phrase: String,
    pub wake_only: bool,
    pub locale: String,
}

impl VoiceConfig {
    pub fn validate(&self) -> Result<(), String> {
        if normalize_phrase(&self.wake_phrase).is_empty() {
            return Err("Wake phrase is required".into());
        }
        if !self.wake_only && normalize_phrase(&self.command_phrase).is_empty() {
            return Err("Combined mode requires a command phrase".into());
        }
        if self.wake_phrase.chars().count() > 80 || self.command_phrase.chars().count() > 80 {
            return Err("Voice phrases must be at most 80 characters".into());
        }
        if self.locale.len() > 32
            || self.locale.is_empty()
            || !self.locale.bytes().all(|ch| ch.is_ascii_alphanumeric() || ch == b'-')
        {
            return Err("Select a recognition locale such as en-US".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub normalized: String,
    pub wake_matched: bool,
    pub command_matched: bool,
    pub would_trigger: bool,
    pub reason: String,
}

#[derive(Default)]
pub struct Matcher {
    last_id: u64,
    last_trigger: Option<Instant>,
    wake_until: Option<Instant>,
}

impl Matcher {
    pub fn decide(
        &mut self,
        config: &VoiceConfig,
        event: &SpeechEvent,
        received: Instant,
        now: Instant,
    ) -> Decision {
        let normalized = normalize_phrase(&event.phrase);
        let wake = normalize_phrase(&config.wake_phrase);
        let command = normalize_phrase(&config.command_phrase);
        if self.wake_until.is_some_and(|until| now > until) {
            self.wake_until = None;
        }
        let waiting_for_command = self.wake_until.is_some();
        let full_phrase = format!("{wake} {command}");
        let mut decision = Decision {
            wake_matched: !wake.is_empty() && (normalized == wake || normalized == full_phrase),
            command_matched: !command.is_empty()
                && (normalized == command || normalized == full_phrase),
            normalized,
            would_trigger: false,
            reason: String::new(),
        };
        let duplicate = event.id == 0 || event.id <= self.last_id;
        if event.final_result {
            self.last_id = self.last_id.max(event.id);
        }
        decision.reason = if let Err(error) = config.validate() {
            error
        } else if event.kind != "result" {
            event
                .error
                .clone()
                .unwrap_or_else(|| "Engine did not return a recognition result".into())
        } else if !event.final_result {
            "Partial recognition".into()
        } else if duplicate {
            "Duplicate or out-of-order result".into()
        } else if now.saturating_duration_since(received) > RESULT_TTL {
            "Recognition result expired".into()
        } else if !event
            .confidence
            .is_some_and(|value| value.is_finite() && (CONFIDENCE_THRESHOLD..=1.0).contains(&value))
        {
            "Low or unavailable confidence".into()
        } else if self
            .last_trigger
            .is_some_and(|last| now.saturating_duration_since(last) < COOLDOWN)
        {
            self.wake_until = None;
            "Trigger cooldown".into()
        } else if config.wake_only {
            if decision.wake_matched {
                self.last_trigger = Some(now);
                decision.would_trigger = true;
                "Accepted wake phrase".into()
            } else {
                "Complete configured phrase required".into()
            }
        } else if decision.normalized == full_phrase {
            self.wake_until = None;
            self.last_trigger = Some(now);
            decision.would_trigger = true;
            "Accepted final command".into()
        } else if waiting_for_command && decision.command_matched {
            self.wake_until = None;
            decision.wake_matched = true;
            self.last_trigger = Some(now);
            decision.would_trigger = true;
            "Accepted command after wake phrase".into()
        } else if decision.wake_matched {
            self.wake_until = Some(now + WAKE_WINDOW);
            "Wake phrase accepted; waiting for command".into()
        } else {
            "Complete configured phrase required".into()
        };
        decision
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpeechEvent {
    pub kind: String,
    pub id: u64,
    pub phrase: String,
    pub confidence: Option<f32>,
    pub final_result: bool,
    pub locale: Option<String>,
    pub microphone: Option<String>,
    pub audio_format: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub generation: Option<String>,
    pub engine: String,
    pub locale: String,
    pub microphone: String,
    pub audio_format: Option<String>,
    pub availability: String,
    pub listening: bool,
    pub test_mode: bool,
    pub error: Option<String>,
}

impl Diagnostics {
    pub fn new(locale: &str) -> Self {
        Self {
            generation: None,
            engine: "System.Speech · constrained grammar".into(),
            locale: locale.into(),
            microphone: "Windows default recording device (independent of clip microphone)".into(),
            audio_format: None,
            availability: if cfg!(windows) { "unchecked" } else { "unavailable" }.into(),
            listening: false,
            test_mode: false,
            error: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionResult {
    pub generation: String,
    pub phrase: String,
    pub confidence: Option<f32>,
    pub decision: Decision,
    pub test_mode: bool,
    pub saved: bool,
}

#[cfg(windows)]
pub const SCRIPT: &str = include_str!("voice.ps1");

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> VoiceConfig {
        VoiceConfig {
            wake_phrase: "Courage".into(),
            command_phrase: "Have it".into(),
            wake_only: false,
            locale: "en-US".into(),
        }
    }
    fn event(phrase: &str) -> SpeechEvent {
        SpeechEvent {
            kind: "result".into(),
            id: 1,
            phrase: phrase.into(),
            confidence: Some(0.95),
            final_result: true,
            ..Default::default()
        }
    }

    #[test]
    fn exact_final_command_only_and_both_modes_agree() {
        let now = Instant::now();
        for (phrase, accepted) in [
            ("Courage, HAVE it!", true),
            ("Courage", false),
            ("have it", false),
            ("handed him", false),
            ("Courage have it please", false),
        ] {
            let decision = Matcher::default().decide(&config(), &event(phrase), now, now);
            assert_eq!(decision.would_trigger, accepted, "{phrase}");
            if phrase == "Courage, HAVE it!" {
                assert!(decision.wake_matched && decision.command_matched);
            }
        }
        let mut config = config();
        config.command_phrase.clear();
        let decision = Matcher::default().decide(&config, &event("Courage"), now, now);
        assert!(!decision.command_matched && !decision.would_trigger);
        config.wake_only = true;
        let decision = Matcher::default().decide(&config, &event("Courage"), now, now);
        assert!(decision.would_trigger && !decision.command_matched);
    }

    #[test]
    fn rejects_partial_low_confidence_expired_and_duplicate_results() {
        let now = Instant::now();
        for confidence in [Some(0.24), Some(0.04), Some(f32::NAN), Some(1.5), None] {
            let mut event = event("Courage have it");
            event.confidence = confidence;
            assert!(!Matcher::default().decide(&config(), &event, now, now).would_trigger);
        }
        let mut result = event("Courage have it");
        result.final_result = false;
        let mut matcher = Matcher::default();
        assert!(!matcher.decide(&config(), &result, now, now).would_trigger);
        result.final_result = true;
        assert!(matcher.decide(&config(), &result, now, now).would_trigger);
        assert!(
            !matcher.decide(&config(), &result, now, now + Duration::from_secs(5)).would_trigger
        );
        result.id = 2;
        assert!(!matcher.decide(&config(), &result, now, now).would_trigger);
        assert!(
            !matcher
                .decide(&config(), &event("Courage have it"), now, now + Duration::from_secs(3))
                .would_trigger
        );
        result.id = 3;
        let later = now + Duration::from_secs(3);
        assert!(matcher.decide(&config(), &result, later, later).would_trigger);
        assert!(!Matcher::default().decide(&config(), &result, now, later).would_trigger);
    }

    #[test]
    fn microphone_and_engine_errors_cannot_trigger() {
        let now = Instant::now();
        for message in ["Microphone unavailable", "No installed recognition engine"] {
            let event: SpeechEvent =
                serde_json::from_value(serde_json::json!({ "kind": "error", "error": message }))
                    .unwrap();
            let decision = Matcher::default().decide(&config(), &event, now, now);
            assert!(!decision.would_trigger);
            assert_eq!(decision.reason, message);
        }
    }

    #[test]
    fn accepts_command_after_a_short_wake_pause() {
        let now = Instant::now();
        let mut matcher = Matcher::default();
        let mut wake = event("Courage");
        wake.id = 1;
        let waiting = matcher.decide(&config(), &wake, now, now);
        assert!(!waiting.would_trigger);
        assert_eq!(waiting.reason, "Wake phrase accepted; waiting for command");

        let mut command = event("have it");
        command.id = 2;
        let accepted = matcher.decide(
            &config(),
            &command,
            now + Duration::from_millis(900),
            now + Duration::from_millis(900),
        );
        assert!(accepted.would_trigger);
        assert!(accepted.wake_matched && accepted.command_matched);
        assert_eq!(accepted.reason, "Accepted command after wake phrase");
    }

    #[test]
    fn expires_a_wake_without_triggering_a_late_command() {
        let now = Instant::now();
        let mut matcher = Matcher::default();
        let mut wake = event("Courage");
        wake.id = 1;
        matcher.decide(&config(), &wake, now, now);

        let mut command = event("have it");
        command.id = 2;
        let decision = matcher.decide(
            &config(),
            &command,
            now + WAKE_WINDOW + Duration::from_millis(1),
            now + WAKE_WINDOW + Duration::from_millis(1),
        );
        assert!(!decision.would_trigger);
        assert_eq!(decision.reason, "Complete configured phrase required");
    }
}
