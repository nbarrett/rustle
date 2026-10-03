#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InsertionPlan {
    pub sentence_continues: bool,
    pub drop_a_trailing_full_stop: bool,
    pub add_a_trailing_space: bool,
    pub add_a_leading_space: bool,
}

pub fn plan_insertion_without_caret_context() -> InsertionPlan {
    InsertionPlan {
        sentence_continues: false,
        drop_a_trailing_full_stop: true,
        add_a_trailing_space: false,
        add_a_leading_space: false,
    }
}

pub fn plan_insertion_for_caret(
    text_before_caret: &str,
    text_after_caret: &str,
    is_a_single_line_field: bool,
) -> InsertionPlan {
    let sentence_continues = caret_text_leaves_a_sentence_open(text_before_caret);
    let field_is_empty = text_before_caret.trim().is_empty() && text_after_caret.trim().is_empty();
    let more_words_follow = text_after_caret
        .chars()
        .any(|character| character.is_alphanumeric());
    let quoted_name = caret_is_inside_unclosed_quotes(text_before_caret)
        && !more_words_follow
        && !text_before_caret
            .chars()
            .rev()
            .skip_while(|character| {
                character.is_whitespace()
                    || matches!(*character, '"' | '\'' | '“' | '”' | '‘' | '’')
            })
            .any(|character| character.is_alphanumeric());
    InsertionPlan {
        sentence_continues,
        add_a_leading_space: text_before_caret.chars().last().is_some_and(|character| {
            character.is_alphanumeric()
                || matches!(
                    character,
                    '.' | '!' | '?' | ',' | ';' | ':' | ')' | ']' | '}'
                )
        }),
        add_a_trailing_space: text_after_caret
            .chars()
            .next()
            .is_some_and(|character| character.is_alphanumeric()),
        drop_a_trailing_full_stop: sentence_continues
            || more_words_follow
            || quoted_name
            || text_after_caret
                .trim_start()
                .chars()
                .next()
                .is_some_and(|character| {
                    matches!(
                        character,
                        '.' | '!' | '?' | ',' | ';' | ':' | ')' | ']' | '}'
                    )
                })
            || (is_a_single_line_field && field_is_empty),
    }
}

fn caret_is_inside_unclosed_quotes(text_before_caret: &str) -> bool {
    let mut double_quotes = 0usize;
    let mut single_quotes = 0usize;
    for character in text_before_caret.chars() {
        match character {
            '"' | '“' | '”' => double_quotes += 1,
            '\'' | '‘' | '’' => single_quotes += 1,
            _ => {}
        }
    }
    double_quotes % 2 == 1 || single_quotes % 2 == 1
}

pub fn fit_transcript_capitalisation_and_punctuation(
    text: &str,
    punctuation: InsertionPlan,
) -> String {
    let without_leading_capital =
        without_a_capital_when_the_sentence_continues(text, punctuation.sentence_continues);
    if punctuation.drop_a_trailing_full_stop {
        without_a_trailing_full_stop(&without_leading_capital)
    } else {
        without_leading_capital
    }
}

pub fn prepare_transcript_for_insertion(text: &str, plan: InsertionPlan) -> String {
    let fitted = fit_transcript_capitalisation_and_punctuation(text.trim(), plan);
    add_spaces_at_insertion_boundaries(&fitted, plan)
}

pub fn add_spaces_at_insertion_boundaries(text: &str, punctuation: InsertionPlan) -> String {
    let mut spaced = text.to_string();
    if punctuation.add_a_leading_space
        && text
            .chars()
            .next()
            .is_some_and(|character| character.is_alphanumeric())
    {
        spaced.insert(0, ' ');
    }
    if punctuation.add_a_trailing_space
        && spaced
            .chars()
            .last()
            .is_some_and(|character| !character.is_whitespace())
    {
        spaced.push(' ');
    }
    spaced
}

pub fn without_a_trailing_full_stop(text: &str) -> String {
    let trimmed = text.trim_end();
    if trimmed.ends_with("...") || trimmed.ends_with('…') {
        return trimmed.to_string();
    }
    if let Some(without_period) = trimmed.strip_suffix('.') {
        if without_period.ends_with('.') {
            return trimmed.to_string();
        }
        return without_period.trim_end().to_string();
    }
    trimmed.to_string()
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
            || matches!(
                character,
                '(' | '[' | '{' | '"' | '\'' | '“' | '‘' | '”' | '’' | ')' | ']' | '}'
            )
        {
            continue;
        }
        return !matches!(character, '.' | '!' | '?' | '…');
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_field_drops_the_whisper_full_stop() {
        let punctuation = plan_insertion_for_caret("", "", true);
        assert!(punctuation.drop_a_trailing_full_stop);
        assert!(!punctuation.sentence_continues);
        assert_eq!(
            fit_transcript_capitalisation_and_punctuation("Rename this thread.", punctuation),
            "Rename this thread"
        );
        assert_eq!(
            fit_transcript_capitalisation_and_punctuation("Is this a question?", punctuation),
            "Is this a question?"
        );
    }

    #[test]
    fn quoted_names_drop_the_whisper_full_stop() {
        let punctuation = plan_insertion_for_caret("\"", "", false);
        assert!(punctuation.drop_a_trailing_full_stop);
        assert_eq!(
            fit_transcript_capitalisation_and_punctuation("Rename this thread.", punctuation),
            "Rename this thread"
        );
    }

    #[test]
    fn a_chat_composer_keeps_the_full_stop() {
        let punctuation = plan_insertion_for_caret("", "", false);
        assert!(!punctuation.drop_a_trailing_full_stop);
        assert_eq!(
            fit_transcript_capitalisation_and_punctuation("Please send the invoice.", punctuation),
            "Please send the invoice."
        );
    }

    #[test]
    fn mid_sentence_dictation_drops_capital_and_full_stop() {
        let punctuation = plan_insertion_for_caret("Please send ", "", false);
        assert!(punctuation.sentence_continues);
        assert!(punctuation.drop_a_trailing_full_stop);
        assert_eq!(
            fit_transcript_capitalisation_and_punctuation("The invoice today.", punctuation),
            "the invoice today"
        );
    }

    #[test]
    fn dictation_in_the_middle_of_existing_words_drops_the_full_stop() {
        let punctuation = plan_insertion_for_caret("Please ", " the invoice.", false);
        assert!(punctuation.drop_a_trailing_full_stop);
        assert_eq!(
            fit_transcript_capitalisation_and_punctuation("Send.", punctuation),
            "send"
        );
    }

    #[test]
    fn a_new_sentence_keeps_capital_and_full_stop() {
        let punctuation = plan_insertion_for_caret("All done. ", "", false);
        assert_eq!(
            punctuation,
            InsertionPlan {
                sentence_continues: false,
                drop_a_trailing_full_stop: false,
                add_a_trailing_space: false,
                add_a_leading_space: false,
            }
        );
        assert_eq!(
            fit_transcript_capitalisation_and_punctuation("Please send the invoice.", punctuation),
            "Please send the invoice."
        );
    }

    #[test]
    fn unavailable_caret_context_drops_the_automatic_full_stop() {
        let punctuation = super::plan_insertion_without_caret_context();
        assert_eq!(
            fit_transcript_capitalisation_and_punctuation("Some inserted words.", punctuation),
            "Some inserted words"
        );
        assert_eq!(
            fit_transcript_capitalisation_and_punctuation("Is this a question?", punctuation),
            "Is this a question?"
        );
        assert_eq!(
            fit_transcript_capitalisation_and_punctuation(
                "First sentence. More words.",
                punctuation
            ),
            "First sentence. More words"
        );
    }

    #[test]
    fn mid_sentence_insertion_separates_the_following_word() {
        let punctuation = plan_insertion_for_caret("Some ", "and more", false);
        let fitted = fit_transcript_capitalisation_and_punctuation("Extra stuff.", punctuation);
        assert_eq!(
            super::add_spaces_at_insertion_boundaries(&fitted, punctuation),
            "extra stuff "
        );
        assert_eq!(
            super::add_spaces_at_insertion_boundaries("extra stuff ", punctuation),
            "extra stuff "
        );
        assert_eq!(
            super::add_spaces_at_insertion_boundaries("", punctuation),
            ""
        );
    }

    #[test]
    fn insertion_does_not_add_space_before_existing_spacing_or_punctuation() {
        for suffix in [" and more", ", and more", ".", ")", "\nmore", ""] {
            let punctuation = plan_insertion_for_caret("Some ", suffix, false);
            assert_eq!(
                super::add_spaces_at_insertion_boundaries("extra stuff", punctuation),
                "extra stuff"
            );
        }
    }

    #[test]
    fn insertion_after_a_word_separates_the_inserted_words() {
        let punctuation = plan_insertion_for_caret("embellish", "", false);
        let fitted = fit_transcript_capitalisation_and_punctuation("Now I add text.", punctuation);
        assert_eq!(
            super::add_spaces_at_insertion_boundaries(&fitted, punctuation),
            " now I add text"
        );
        assert_eq!(
            super::add_spaces_at_insertion_boundaries(" already spaced", punctuation),
            " already spaced"
        );
        assert_eq!(
            super::add_spaces_at_insertion_boundaries("", punctuation),
            ""
        );
    }

    #[test]
    fn insertion_between_words_separates_both_boundaries() {
        let punctuation = plan_insertion_for_caret("embellish", "now", false);
        assert_eq!(
            super::add_spaces_at_insertion_boundaries("some words", punctuation),
            " some words "
        );
    }

    #[test]
    fn insertion_keeps_existing_leading_spacing_and_attached_punctuation() {
        for prefix in ["embellish ", "", "(", "\n"] {
            let punctuation = plan_insertion_for_caret(prefix, "", false);
            assert_eq!(
                super::add_spaces_at_insertion_boundaries("some words", punctuation),
                "some words"
            );
        }
        let punctuation = plan_insertion_for_caret("embellish", "", false);
        assert_eq!(
            super::add_spaces_at_insertion_boundaries(", some words", punctuation),
            ", some words"
        );
    }

    #[test]
    fn insertion_matrix_preserves_the_complete_surrounding_sentence() {
        let cases = [
            ("", "", false, "A new sentence.", "A new sentence."),
            ("", "", true, "A title.", "A title"),
            (
                "Please ",
                " the invoice.",
                false,
                "Send.",
                "Please send the invoice.",
            ),
            (
                "Please",
                " the invoice.",
                false,
                "Send.",
                "Please send the invoice.",
            ),
            (
                "Please ",
                "the invoice.",
                false,
                "Send.",
                "Please send the invoice.",
            ),
            (
                "Please",
                "the invoice.",
                false,
                "Send.",
                "Please send the invoice.",
            ),
            ("Done.", "", false, "Next sentence.", "Done. Next sentence."),
            (
                "Done. ",
                "",
                false,
                "Next sentence.",
                "Done. Next sentence.",
            ),
            (
                "He said ",
                ", then left.",
                false,
                "Hello.",
                "He said hello, then left.",
            ),
            ("Please ", ".", false, "Send it.", "Please send it."),
            ("(Please ", ")", false, "Send it.", "(Please send it)"),
            (
                "First line.\n",
                "",
                false,
                "Second line.",
                "First line.\nSecond line.",
            ),
            (
                "Please ",
                " tomorrow.",
                false,
                "I will send it.",
                "Please I will send it tomorrow.",
            ),
            ("Use ", " today.", false, "HTTP.", "Use HTTP today."),
            (
                "",
                " existing words.",
                false,
                "More.",
                "More existing words.",
            ),
            ("prefix ", " suffix", false, "", "prefix  suffix"),
        ];
        for (before, after, single_line, dictated, expected) in cases {
            let plan = plan_insertion_for_caret(before, after, single_line);
            let inserted = prepare_transcript_for_insertion(dictated, plan);
            assert_eq!(
                format!("{before}{inserted}{after}"),
                expected,
                "before={before:?} after={after:?} dictated={dictated:?}"
            );
            assert_eq!(prepare_transcript_for_insertion(&inserted, plan), inserted);
        }
    }

    #[test]
    fn repeated_dictation_uses_the_updated_sentence_context() {
        let mut field = String::new();
        for dictated in ["Please send.", "The invoice.", "Today."] {
            let plan = plan_insertion_for_caret(&field, "", false);
            field.push_str(&prepare_transcript_for_insertion(dictated, plan));
        }
        assert_eq!(field, "Please send. The invoice. Today.");
        let mut unfinished = "Please".to_string();
        for dictated in ["Send.", "The invoice.", "Today."] {
            let plan = plan_insertion_for_caret(&unfinished, "", false);
            unfinished.push_str(&prepare_transcript_for_insertion(dictated, plan));
        }
        assert_eq!(unfinished, "Please send the invoice today");
    }
}
