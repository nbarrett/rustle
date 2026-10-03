use crate::config::{apply_corrections, Correction};
use crate::uk_english::apply_uk_spellings;

pub fn is_nonspeech_annotation(segment: &str) -> bool {
    (segment.starts_with('[') && segment.ends_with(']'))
        || (segment.starts_with('(') && segment.ends_with(')'))
}

const WHISPER_OUTRO_PHRASES: &[&str] = &[
    "thank you very much",
    "thank you so much",
    "thank you thank you",
    "thanks so much",
    "thanks a lot",
    "thanks thanks",
    "thank you",
    "thanks",
    "see you next week",
    "see you next time",
    "see you later",
    "see you soon",
    "see ya later",
    "see ya soon",
    "good bye",
    "goodbye",
    "bye bye",
    "okay bye",
    "ok bye",
    "take care",
    "see you",
    "see ya",
    "cheers",
    "okay",
    "cya",
    "bye",
    "ok",
    "i am sorry",
    "i m sorry",
    "sorry",
];

pub fn transcript_is_only_thanks(text: &str) -> bool {
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
    )
}

fn transcript_is_only_a_whisper_farewell(text: &str) -> bool {
    matches!(
        normalised_transcript_words(text).as_str(),
        "bye"
            | "goodbye"
            | "good bye"
            | "bye bye"
            | "see you"
            | "see ya"
            | "see ya later"
            | "see ya soon"
            | "cya"
            | "see you soon"
            | "see you later"
            | "see you next week"
            | "see you next time"
            | "take care"
            | "cheers"
            | "ok"
            | "okay"
            | "ok bye"
            | "okay bye"
    )
}

fn transcript_is_only_a_whisper_apology(text: &str) -> bool {
    matches!(
        normalised_transcript_words(text).as_str(),
        "sorry" | "i m sorry" | "i am sorry"
    )
}

fn words_are_only_whisper_outro_phrases(words: &str) -> bool {
    let mut remaining = words.trim();
    if remaining.is_empty() {
        return false;
    }
    while !remaining.is_empty() {
        let matched = WHISPER_OUTRO_PHRASES.iter().find(|phrase| {
            remaining == **phrase
                || remaining
                    .strip_prefix(*phrase)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
        });
        let Some(phrase) = matched else {
            return false;
        };
        remaining = remaining[phrase.len()..].trim_start();
    }
    true
}

pub fn transcript_is_only_a_whisper_outro(text: &str) -> bool {
    transcript_is_only_thanks(text)
        || transcript_is_only_a_whisper_farewell(text)
        || transcript_is_only_a_whisper_apology(text)
        || words_are_only_whisper_outro_phrases(&normalised_transcript_words(text))
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
    if transcript_is_only_a_whisper_outro(spoken) && spoken_words != live_words {
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

fn strip_one_trailing_whisper_outro(text: &str) -> Option<String> {
    let without_end_marks = text.trim_end_matches(|character: char| {
        matches!(character, '.' | '!' | '?' | ',' | ';' | ':') || character.is_whitespace()
    });
    let boundary =
        without_end_marks.rfind(|character: char| matches!(character, '.' | '!' | '?'))?;
    let tail = without_end_marks[boundary + 1..].trim();
    if tail.is_empty() || !transcript_is_a_trailing_whisper_outro(tail) {
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

pub fn transcript_is_a_trailing_whisper_outro(text: &str) -> bool {
    transcript_is_only_a_whisper_outro(text) || transcript_is_a_whisper_blank_phrase(text)
}

pub fn live_preview_only_adds_a_whisper_outro(previous: &str, next: &str) -> bool {
    let previous_words = normalised_transcript_words(previous);
    let next_words = normalised_transcript_words(next);
    if previous_words.is_empty() || !next_words.starts_with(&previous_words) {
        return false;
    }
    let extra = next_words
        .strip_prefix(&previous_words)
        .map(str::trim)
        .unwrap_or("");
    !extra.is_empty() && transcript_is_a_trailing_whisper_outro(extra)
}

pub fn without_trailing_whisper_outros(text: &str) -> String {
    let mut current = text.trim_end().to_string();
    while let Some(stripped) = strip_one_trailing_whisper_outro(&current) {
        if stripped == current {
            break;
        }
        current = stripped;
    }
    current
}

pub fn transcript_is_a_whisper_blank_phrase(text: &str) -> bool {
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

pub fn without_whisper_silence_ellipses(text: &str) -> String {
    let mut current = text.replace('…', "...");
    while current.contains("...") {
        current = current.replace("...", " ");
    }
    current.split_whitespace().collect::<Vec<_>>().join(" ")
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
    let without_hallucinations = without_trailing_whisper_outros(corrected.trim());
    let spoken = without_whisper_silence_ellipses(without_hallucinations.trim());
    if spoken.is_empty() || transcript_is_a_whisper_blank_phrase(&spoken) {
        return None;
    }
    Some(spoken.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        final_pass_only_extends_the_spoken_words, final_pass_threw_away_the_spoken_words,
        live_preview_only_adds_a_whisper_outro, normalised_transcript_words,
        transcript_is_a_whisper_blank_phrase, transcript_is_only_a_whisper_outro,
        transcript_ready_for_the_clipboard, without_trailing_whisper_outros,
        without_whisper_silence_ellipses,
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
        assert!(!transcript_is_a_whisper_blank_phrase("Thank you."));
        assert!(!transcript_is_a_whisper_blank_phrase("thanks"));
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
    fn a_trailing_thank_you_sentence_is_stripped_without_live_speech() {
        assert_eq!(
            without_trailing_whisper_outros("See what I mean about the thank you ?. Thank you."),
            "See what I mean about the thank you ?"
        );
        assert_eq!(
            without_trailing_whisper_outros("Hello. Thank you."),
            "Hello."
        );
        assert_eq!(
            without_trailing_whisper_outros("Please send the invoice, thank you"),
            "Please send the invoice, thank you"
        );
        assert_eq!(without_trailing_whisper_outros("Thank you."), "Thank you.");
        assert_eq!(without_trailing_whisper_outros("I'm sorry."), "I'm sorry.");
        assert_eq!(
            without_trailing_whisper_outros("That's all. Thanks for watching."),
            "That's all."
        );
        assert_eq!(
            without_trailing_whisper_outros("Do it like the other one. Bye."),
            "Do it like the other one."
        );
        assert_eq!(
            without_trailing_whisper_outros("Book the walk. See you next week."),
            "Book the walk."
        );
        assert_eq!(
            without_trailing_whisper_outros("Book the walk. See ya."),
            "Book the walk."
        );
        assert_eq!(
            without_trailing_whisper_outros("Book the walk. See ya later."),
            "Book the walk."
        );
        assert_eq!(
            without_trailing_whisper_outros("See what happens. OK."),
            "See what happens."
        );
        assert_eq!(
            without_trailing_whisper_outros("See what happens. Okay."),
            "See what happens."
        );
        assert_eq!(
            without_trailing_whisper_outros(
                "Not sure what he means about Sarah, she should be available. I'm sorry. I'm sorry. I'm sorry. I'm sorry. I'm sorry. Thank you."
            ),
            "Not sure what he means about Sarah, she should be available."
        );
        assert_eq!(
            without_trailing_whisper_outros(
                "She should be available. I'm sorry I'm sorry Thank you."
            ),
            "She should be available."
        );
    }

    #[test]
    fn a_lone_thank_you_is_kept_as_spoken_words() {
        assert!(transcript_is_only_a_whisper_outro("Thank you."));
        assert!(transcript_is_only_a_whisper_outro("Thanks!"));
        assert!(transcript_is_only_a_whisper_outro("Thank you so much"));
        assert!(!transcript_is_a_whisper_blank_phrase("Thank you."));
        assert!(!transcript_is_a_whisper_blank_phrase("Thanks"));
        assert!(!transcript_is_a_whisper_blank_phrase("Bye."));
        assert!(transcript_is_only_a_whisper_outro("Goodbye"));
        assert!(transcript_is_only_a_whisper_outro("See you next week"));
        assert!(transcript_is_only_a_whisper_outro("See ya"));
        assert!(transcript_is_only_a_whisper_outro("See ya later."));
        assert!(!transcript_is_only_a_whisper_outro(
            "Please send the invoice, thank you"
        ));
        assert!(!transcript_is_a_whisper_blank_phrase(
            "Please send the invoice, thank you"
        ));
        assert_eq!(
            transcript_ready_for_the_clipboard("Thank you.", &[], false),
            Some("Thank you.".to_string())
        );
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
    fn live_preview_does_not_append_a_silence_okay() {
        assert!(live_preview_only_adds_a_whisper_outro(
            "See what happens.",
            "See what happens. OK."
        ));
        assert!(!live_preview_only_adds_a_whisper_outro(
            "See what happens.",
            "See what happens next."
        ));
    }

    #[test]
    fn whisper_silence_ellipses_are_removed() {
        assert_eq!(
            without_whisper_silence_ellipses("And... ... ... ... Lots of ellipses get put in."),
            "And Lots of ellipses get put in."
        );
        assert_eq!(
            without_whisper_silence_ellipses("the things that…"),
            "the things that"
        );
        assert_eq!(without_whisper_silence_ellipses("... ... ..."), "");
        assert_eq!(
            without_whisper_silence_ellipses("Hello. World"),
            "Hello. World"
        );
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
