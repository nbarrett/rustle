use anyhow::{anyhow, Result};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct WhisperTranscriber {
    context: WhisperContext,
}

impl WhisperTranscriber {
    pub fn load_from_path(model_path: &str) -> Result<Self> {
        let context =
            WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
                .map_err(|error| anyhow!("failed to load model at {model_path}: {error}"))?;
        Ok(Self { context })
    }

    pub fn transcribe_the_whole_clip(&self, audio: &[f32]) -> Result<String> {
        if audio.is_empty() {
            return Ok(String::new());
        }
        let mut state = self.context.create_state()?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_no_context(true);
        params.set_single_segment(false);
        params.set_suppress_blank(true);
        params.set_suppress_non_speech_tokens(true);
        let british_prompt = crate::uk_english::prefers_british_english()
            .then_some("British English spelling.");
        if let Some(prompt) = british_prompt {
            params.set_initial_prompt(prompt);
        }

        state.full(params, audio)?;

        let segment_count = state.full_n_segments()?;
        let mut transcript = String::new();
        for segment_index in 0..segment_count {
            let segment = state.full_get_segment_text(segment_index)?;
            if is_nonspeech_annotation(segment.trim()) {
                continue;
            }
            transcript.push_str(&segment);
        }
        Ok(transcript)
    }
}

fn is_nonspeech_annotation(segment: &str) -> bool {
    (segment.starts_with('[') && segment.ends_with(']'))
        || (segment.starts_with('(') && segment.ends_with(')'))
}

pub fn transcript_is_only_thank_you(text: &str) -> bool {
    matches!(
        normalised_transcript_words(text).as_str(),
        "thank you"
            | "thanks"
            | "thank you so much"
            | "thanks so much"
            | "thank you very much"
            | "thanks a lot"
            | "thank you thank you"
            | "thanks thanks"
            | "bye"
            | "goodbye"
            | "good bye"
            | "bye bye"
    )
}

pub fn final_pass_threw_away_the_spoken_words(live: &str, spoken: &str) -> bool {
    let live_words = normalised_transcript_words(live);
    let spoken_words = normalised_transcript_words(spoken);
    if live_words.is_empty() {
        return false;
    }
    if spoken_words.is_empty() {
        return true;
    }
    if transcript_is_a_whisper_blank_phrase(spoken) {
        return true;
    }
    if transcript_is_only_thank_you(spoken) && spoken_words != live_words {
        return true;
    }
    live_words.starts_with(&spoken_words) && live_words.len() > spoken_words.len()
}

pub fn final_pass_only_extends_the_spoken_words(live: &str, spoken: &str) -> bool {
    let live_words = normalised_transcript_words(live);
    let spoken_words = normalised_transcript_words(spoken);
    !live_words.is_empty()
        && spoken_words.starts_with(&live_words)
        && spoken_words.len() >= live_words.len()
}

pub fn without_a_trailing_whisper_thank_you(text: &str) -> String {
    let mut current = text.trim_end().to_string();
    loop {
        let Some(stripped) = strip_one_trailing_hallucinated_thank_you(&current) else {
            break;
        };
        if stripped == current {
            break;
        }
        current = stripped;
    }
    current
}

fn strip_one_trailing_hallucinated_thank_you(text: &str) -> Option<String> {
    let without_end_marks = text.trim_end_matches(|character: char| {
        matches!(character, '.' | '!' | '?' | ',' | ';' | ':') || character.is_whitespace()
    });
    let boundary = without_end_marks.rfind(|character: char| matches!(character, '.' | '!' | '?'))?;
    let tail = without_end_marks[boundary + 1..].trim();
    if tail.is_empty() || !transcript_is_a_whisper_blank_phrase(tail) {
        return None;
    }
    let head = without_end_marks[..=boundary].trim_end();
    if head.is_empty() {
        return None;
    }
    Some(without_a_period_stacked_on_another_end_mark(head))
}

fn without_a_period_stacked_on_another_end_mark(text: &str) -> String {
    let trimmed = text.trim_end();
    if let Some(without_period) = trimmed.strip_suffix('.') {
        if without_period.ends_with('?') || without_period.ends_with('!') {
            return without_period.trim_end().to_string();
        }
    }
    trimmed.to_string()
}

pub fn transcript_is_a_whisper_blank_phrase(text: &str) -> bool {
    if transcript_is_only_thank_you(text) {
        return true;
    }
    let normalised = normalised_transcript_words(text);
    if normalised.is_empty() {
        return text.trim().is_empty();
    }
    matches!(
        normalised.as_str(),
        "thanks for watching"
            | "thank you for watching"
            | "thanks for watching please subscribe"
            | "thank you for watching please subscribe"
            | "please subscribe"
            | "like and subscribe"
            | "thanks for listening"
            | "thank you for listening"
            | "the end"
            | "music"
            | "applause"
            | "silence"
            | "subtitle"
            | "subtitles"
            | "bye"
            | "goodbye"
            | "good bye"
            | "bye bye"
    ) || normalised.starts_with("thanks for watching")
        || normalised.starts_with("thank you for watching")
        || normalised.starts_with("subtitles by")
}

fn normalised_transcript_words(text: &str) -> String {
    let mut words = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            current.push(character.to_ascii_lowercase());
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::{
        final_pass_only_extends_the_spoken_words, final_pass_threw_away_the_spoken_words,
        normalised_transcript_words, transcript_is_a_whisper_blank_phrase,
        transcript_is_only_thank_you, without_a_trailing_whisper_thank_you,
    };

    #[test]
    fn youtube_credit_lines_are_blank_phrases() {
        assert!(transcript_is_a_whisper_blank_phrase("Thanks for watching!"));
        assert!(transcript_is_a_whisper_blank_phrase("Please subscribe"));
        assert!(transcript_is_a_whisper_blank_phrase(""));
        assert!(transcript_is_a_whisper_blank_phrase("   "));
        assert!(transcript_is_a_whisper_blank_phrase("Thank you."));
        assert!(transcript_is_a_whisper_blank_phrase("thanks"));
        assert!(!transcript_is_a_whisper_blank_phrase("hold the function key"));
        assert!(!transcript_is_a_whisper_blank_phrase("(, )."));
        assert!(!transcript_is_a_whisper_blank_phrase("?"));
    }

    #[test]
    fn normalised_transcript_drops_punctuation() {
        assert_eq!(
            normalised_transcript_words("Thank you."),
            "thank you"
        );
    }

    #[test]
    fn a_trailing_thank_you_sentence_is_stripped() {
        assert_eq!(
            without_a_trailing_whisper_thank_you(
                "See what I mean about the thank you ?. Thank you."
            ),
            "See what I mean about the thank you ?"
        );
        assert_eq!(
            without_a_trailing_whisper_thank_you("Hello. Thank you."),
            "Hello."
        );
        assert_eq!(
            without_a_trailing_whisper_thank_you("Please send the invoice, thank you"),
            "Please send the invoice, thank you"
        );
        assert_eq!(
            without_a_trailing_whisper_thank_you("Thank you."),
            "Thank you."
        );
        assert_eq!(
            without_a_trailing_whisper_thank_you("That's all. Thanks for watching."),
            "That's all."
        );
        assert_eq!(
            without_a_trailing_whisper_thank_you("Do it like the other one. Bye."),
            "Do it like the other one."
        );
    }

    #[test]
    fn a_lone_thank_you_is_treated_as_a_whisper_hallucination() {
        assert!(transcript_is_only_thank_you("Thank you."));
        assert!(transcript_is_only_thank_you("Thanks!"));
        assert!(transcript_is_only_thank_you("Thank you so much"));
        assert!(transcript_is_a_whisper_blank_phrase("Thank you."));
        assert!(transcript_is_a_whisper_blank_phrase("Thanks"));
        assert!(transcript_is_a_whisper_blank_phrase("Bye."));
        assert!(transcript_is_only_thank_you("Goodbye"));
        assert!(!transcript_is_only_thank_you("Please send the invoice, thank you"));
        assert!(!transcript_is_a_whisper_blank_phrase(
            "Please send the invoice, thank you"
        ));
    }

    #[test]
    fn final_pass_must_not_delete_a_trailing_thank_you() {
        assert!(final_pass_threw_away_the_spoken_words(
            "Please send the invoice, thank you",
            "Please send the invoice"
        ));
        assert!(final_pass_threw_away_the_spoken_words(
            "Please send the invoice, thank you",
            "Thank you."
        ));
        assert!(!final_pass_threw_away_the_spoken_words(
            "Please send the invoice",
            "Please send the invoice, thank you"
        ));
        assert!(!final_pass_threw_away_the_spoken_words("", "Thank you."));
    }

    #[test]
    fn final_pass_can_add_the_last_words_without_rewriting() {
        assert!(final_pass_only_extends_the_spoken_words(
            "Pointing towards all the least notes",
            "Pointing towards all the least notes to do with the email."
        ));
        assert!(!final_pass_only_extends_the_spoken_words(
            "Pointing towards all the least notes",
            "Please send the invoice"
        ));
    }
}
