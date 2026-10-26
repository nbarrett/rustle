use anyhow::{anyhow, Result};

use crate::config::Correction;
use crate::transcript::is_nonspeech_annotation;
pub use crate::transcript::{
    final_pass_only_extends_the_spoken_words, final_pass_threw_away_the_spoken_words,
    transcript_is_a_whisper_blank_phrase, transcript_is_only_a_whisper_outro,
    transcript_is_only_thanks, without_trailing_whisper_outros,
};
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
        self.transcribe_the_live_clip_with_vocabulary(audio, &[])
    }

    pub fn transcribe_the_live_clip_with_vocabulary(
        &self,
        audio: &[f32],
        corrections: &[Correction],
    ) -> Result<String> {
        self.transcribe_with_sampling_strategy(
            audio,
            SamplingStrategy::Greedy { best_of: 1 },
            corrections,
        )
    }

    pub fn transcribe_the_finished_clip(&self, audio: &[f32]) -> Result<String> {
        self.transcribe_the_finished_clip_with_vocabulary(audio, &[])
    }

    pub fn transcribe_the_finished_clip_with_vocabulary(
        &self,
        audio: &[f32],
        corrections: &[Correction],
    ) -> Result<String> {
        self.transcribe_with_sampling_strategy(
            audio,
            SamplingStrategy::Greedy { best_of: 1 },
            corrections,
        )
    }

    fn transcribe_with_sampling_strategy(
        &self,
        audio: &[f32],
        sampling_strategy: SamplingStrategy,
        corrections: &[Correction],
    ) -> Result<String> {
        if audio.is_empty() {
            return Ok(String::new());
        }
        let mut state = self.context.create_state()?;

        let mut params = FullParams::new(sampling_strategy);
        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_no_context(true);
        params.set_single_segment(false);
        params.set_suppress_blank(true);
        params.set_suppress_non_speech_tokens(true);
        let prompt =
            build_transcription_prompt(crate::uk_english::prefers_british_english(), corrections);
        if !prompt.is_empty() {
            params.set_initial_prompt(&prompt);
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

fn build_transcription_prompt(british_english: bool, corrections: &[Correction]) -> String {
    let mut prompt = if british_english {
        "British English spelling.".to_string()
    } else {
        String::new()
    };
    let mut vocabulary = Vec::new();
    let mut character_count = prompt.chars().count();
    for correction in corrections.iter().rev() {
        let written = correction.written.trim();
        if written.is_empty() || vocabulary.contains(&written) {
            continue;
        }
        let added_characters = written.chars().count() + 2;
        if character_count + added_characters > 600 {
            continue;
        }
        vocabulary.push(written);
        character_count += added_characters;
    }
    if !vocabulary.is_empty() {
        if !prompt.is_empty() {
            prompt.push(' ');
        }
        prompt.push_str("Vocabulary: ");
        prompt.push_str(&vocabulary.join(", "));
        prompt.push('.');
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::build_transcription_prompt;
    use crate::config::Correction;

    #[test]
    fn recognition_vocabulary_uses_corrected_spellings_without_spoken_mishearings() {
        let prompt = build_transcription_prompt(
            true,
            &[Correction {
                spoken: "Shoo berry Ness".into(),
                written: "Shoeburyness".into(),
            }],
        );
        assert!(prompt.contains("British English spelling."));
        assert!(prompt.contains("Shoeburyness"));
        assert!(!prompt.contains("Shoo berry Ness"));
        assert_eq!(build_transcription_prompt(false, &[]), "");
    }

    #[test]
    fn recognition_vocabulary_prioritises_recent_corrections_with_a_bounded_prompt() {
        let mut corrections: Vec<_> = (0..100)
            .map(|index| Correction {
                spoken: index.to_string(),
                written: format!("Corrected name {index}"),
            })
            .collect();
        corrections.push(Correction {
            spoken: "latest".into(),
            written: "🦀 Shoeburyness".into(),
        });
        let prompt = build_transcription_prompt(false, &corrections);
        assert!(prompt.starts_with("Vocabulary: 🦀 Shoeburyness"));
        assert!(prompt.chars().count() < 620);
    }
}
