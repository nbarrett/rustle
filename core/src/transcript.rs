use crate::config::{apply_corrections, Correction};
use crate::uk_english::apply_uk_spellings;

pub fn is_nonspeech_annotation(segment: &str) -> bool {
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
    let boundary =
        without_end_marks.rfind(|character: char| matches!(character, '.' | '!' | '?'))?;
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

pub fn without_a_capital_when_the_sentence_continues(
    text: &str,
    sentence_continues: bool,
) -> String {
    if !sentence_continues {
        return text.to_string();
    }
    let Some(first_letter_index) = text.find(|character: char| character.is_alphabetic()) else {
        return text.to_string();
    };
    let first_word: String = text[first_letter_index..]
        .chars()
        .take_while(|character| !character.is_whitespace())
        .collect();
    if first_word_must_keep_its_capital(&first_word) {
        return text.to_string();
    }
    let mut characters = text[first_letter_index..].chars();
    let Some(first_letter) = characters.next() else {
        return text.to_string();
    };
    format!(
        "{}{}{}",
        &text[..first_letter_index],
        first_letter.to_lowercase(),
        characters.as_str()
    )
}

pub fn first_word_must_keep_its_capital(word: &str) -> bool {
    let trimmed = word.trim_end_matches(|character: char| !character.is_alphanumeric());
    if trimmed == "I" || trimmed.starts_with("I'") || trimmed.starts_with("I’") {
        return true;
    }
    trimmed
        .chars()
        .skip(1)
        .any(|character| character.is_uppercase())
}

pub fn caret_text_leaves_a_sentence_open(text_before_caret: &str) -> bool {
    for character in text_before_caret.chars().rev() {
        if character == '\n' || character == '\r' {
            return false;
        }
        if character.is_whitespace()
            || matches!(character, '(' | '[' | '{' | '"' | '\'' | '“' | '‘')
        {
            continue;
        }
        return !matches!(character, '.' | '!' | '?' | '…');
    }
    false
}

pub fn without_trailing_ellipsis(text: &str) -> &str {
    text.trim_end()
        .trim_end_matches("...")
        .trim_end_matches('…')
        .trim_end()
}

pub fn transcript_ready_for_the_clipboard(
    raw_transcript: &str,
    corrections: &[Correction],
    prefers_british_spelling: bool,
) -> Option<String> {
    let trimmed_source = raw_transcript.trim();
    let regional = if prefers_british_spelling {
        apply_uk_spellings(trimmed_source)
    } else {
        trimmed_source.to_string()
    };
    let corrected = apply_corrections(&regional, corrections);
    let without_hallucinations = without_a_trailing_whisper_thank_you(corrected.trim());
    let spoken = without_trailing_ellipsis(without_hallucinations.trim()).trim();
    if spoken.is_empty() || transcript_is_a_whisper_blank_phrase(spoken) {
        return None;
    }
    Some(spoken.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        final_pass_only_extends_the_spoken_words, final_pass_threw_away_the_spoken_words,
        normalised_transcript_words, transcript_is_a_whisper_blank_phrase,
        transcript_is_only_thank_you, transcript_ready_for_the_clipboard,
        without_a_trailing_whisper_thank_you,
    };
    use crate::config::Correction;

    fn rustle_correction() -> Vec<Correction> {
        vec![Correction {
            spoken: "russell".to_string(),
            written: "Rustle".to_string(),
        }]
    }

    #[test]
    fn clipboard_pass_applies_corrections() {
        assert_eq!(
            transcript_ready_for_the_clipboard("russell works well.", &rustle_correction(), false),
            Some("Rustle works well.".to_string())
        );
    }

    #[test]
    fn clipboard_pass_applies_british_spelling_only_when_asked() {
        assert_eq!(
            transcript_ready_for_the_clipboard("Please summarize this.", &[], true),
            Some("Please summarise this.".to_string())
        );
        assert_eq!(
            transcript_ready_for_the_clipboard("Please summarize this.", &[], false),
            Some("Please summarize this.".to_string())
        );
    }

    #[test]
    fn clipboard_pass_speaks_punctuation() {
        assert_eq!(
            transcript_ready_for_the_clipboard("left bracket 87 right bracket", &[], false),
            Some("(87)".to_string())
        );
    }

    #[test]
    fn clipboard_pass_drops_whisper_hallucinations() {
        assert_eq!(
            transcript_ready_for_the_clipboard("Thanks for watching!", &[], false),
            None
        );
        assert_eq!(transcript_ready_for_the_clipboard("   ", &[], false), None);
    }

    #[test]
    fn clipboard_pass_strips_a_trailing_thank_you() {
        assert_eq!(
            transcript_ready_for_the_clipboard("Book the walk for Tuesday. Thank you.", &[], false),
            Some("Book the walk for Tuesday.".to_string())
        );
    }

    #[test]
    fn youtube_credit_lines_are_blank_phrases() {
        assert!(transcript_is_a_whisper_blank_phrase("Thanks for watching!"));
        assert!(transcript_is_a_whisper_blank_phrase("Please subscribe"));
        assert!(transcript_is_a_whisper_blank_phrase(""));
        assert!(transcript_is_a_whisper_blank_phrase("   "));
        assert!(transcript_is_a_whisper_blank_phrase("Thank you."));
        assert!(transcript_is_a_whisper_blank_phrase("thanks"));
        assert!(!transcript_is_a_whisper_blank_phrase(
            "hold the function key"
        ));
        assert!(!transcript_is_a_whisper_blank_phrase("(, )."));
        assert!(!transcript_is_a_whisper_blank_phrase("?"));
    }

    #[test]
    fn normalised_transcript_drops_punctuation() {
        assert_eq!(normalised_transcript_words("Thank you."), "thank you");
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
        assert!(!transcript_is_only_thank_you(
            "Please send the invoice, thank you"
        ));
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
