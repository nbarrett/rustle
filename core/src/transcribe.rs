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

    pub fn transcribe(&self, audio: &[f32]) -> Result<String> {
        let mut state = self.context.create_state()?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);

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
