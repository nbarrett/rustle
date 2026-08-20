use std::sync::OnceLock;

const AMERICAN_TO_BRITISH: &[(&str, &str)] = &[
    ("analyze", "analyse"),
    ("analyzed", "analysed"),
    ("analyzes", "analyses"),
    ("analyzing", "analysing"),
    ("behavior", "behaviour"),
    ("behaviors", "behaviours"),
    ("behavioral", "behavioural"),
    ("caliber", "calibre"),
    ("canceled", "cancelled"),
    ("canceling", "cancelling"),
    ("catalog", "catalogue"),
    ("catalogs", "catalogues"),
    ("center", "centre"),
    ("centered", "centred"),
    ("centering", "centring"),
    ("centers", "centres"),
    ("color", "colour"),
    ("colored", "coloured"),
    ("colorful", "colourful"),
    ("coloring", "colouring"),
    ("colors", "colours"),
    ("counselor", "counsellor"),
    ("defense", "defence"),
    ("defenses", "defences"),
    ("encyclopedia", "encyclopaedia"),
    ("endeavor", "endeavour"),
    ("endeavors", "endeavours"),
    ("favor", "favour"),
    ("favorite", "favourite"),
    ("favorites", "favourites"),
    ("favors", "favours"),
    ("fiber", "fibre"),
    ("fibers", "fibres"),
    ("flavor", "flavour"),
    ("flavors", "flavours"),
    ("fulfill", "fulfil"),
    ("fulfillment", "fulfilment"),
    ("gray", "grey"),
    ("honor", "honour"),
    ("honorable", "honourable"),
    ("honored", "honoured"),
    ("honoring", "honouring"),
    ("honors", "honours"),
    ("humor", "humour"),
    ("humorous", "humorous"),
    ("initialize", "initialise"),
    ("initialized", "initialised"),
    ("initializes", "initialises"),
    ("initializing", "initialising"),
    ("jewelry", "jewellery"),
    ("labeled", "labelled"),
    ("labeling", "labelling"),
    ("labor", "labour"),
    ("liter", "litre"),
    ("liters", "litres"),
    ("minimize", "minimise"),
    ("minimized", "minimised"),
    ("minimizes", "minimises"),
    ("minimizing", "minimising"),
    ("neighbor", "neighbour"),
    ("neighborhood", "neighbourhood"),
    ("neighbors", "neighbours"),
    ("offense", "offence"),
    ("offenses", "offences"),
    ("optimize", "optimise"),
    ("optimized", "optimised"),
    ("optimizes", "optimises"),
    ("optimizing", "optimising"),
    ("organization", "organisation"),
    ("organizational", "organisational"),
    ("organizations", "organisations"),
    ("organize", "organise"),
    ("organized", "organised"),
    ("organizes", "organises"),
    ("organizing", "organising"),
    ("recognize", "recognise"),
    ("recognized", "recognised"),
    ("recognizes", "recognises"),
    ("recognizing", "recognising"),
    ("rumor", "rumour"),
    ("rumors", "rumours"),
    ("signaling", "signalling"),
    ("skillful", "skilful"),
    ("standardize", "standardise"),
    ("standardized", "standardised"),
    ("summarize", "summarise"),
    ("summarized", "summarised"),
    ("summarizes", "summarises"),
    ("summarizing", "summarising"),
    ("theater", "theatre"),
    ("theaters", "theatres"),
    ("traveling", "travelling"),
    ("traveled", "travelled"),
    ("utilize", "utilise"),
    ("utilized", "utilised"),
    ("utilizes", "utilises"),
    ("utilizing", "utilising"),
];

pub fn prefers_british_english() -> bool {
    static PREFERS: OnceLock<bool> = OnceLock::new();
    *PREFERS.get_or_init(detect_british_english)
}

fn detect_british_english() -> bool {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::NSLocale;
        let locale = NSLocale::currentLocale();
        if let Some(region) = locale.regionCode() {
            let region = region.to_string();
            if region.eq_ignore_ascii_case("GB") || region.eq_ignore_ascii_case("IE") {
                return true;
            }
        }
        let identifier = locale.localeIdentifier().to_string();
        if identifier_is_british(&identifier) {
            return true;
        }
        for language in NSLocale::preferredLanguages() {
            if identifier_is_british(&language.to_string()) {
                return true;
            }
        }
    }
    std::env::var("LANG")
        .ok()
        .is_some_and(|lang| identifier_is_british(&lang))
}

fn identifier_is_british(tag: &str) -> bool {
    let normalised = tag.replace('_', "-").to_ascii_lowercase();
    normalised.starts_with("en-gb")
        || normalised.starts_with("en-ie")
        || normalised.contains("-gb.")
        || normalised.contains("-gb_")
}

pub fn apply_uk_spellings(text: &str) -> String {
    let mut result = text.to_string();
    for (american, british) in AMERICAN_TO_BRITISH {
        result = replace_whole_word_preserving_case(&result, american, british);
    }
    result
}

fn replace_whole_word_preserving_case(haystack: &str, from: &str, to: &str) -> String {
    let lower_haystack = haystack.to_ascii_lowercase();
    let lower_from = from.to_ascii_lowercase();
    let bytes = lower_haystack.as_bytes();
    let mut result = String::with_capacity(haystack.len());
    let mut index = 0;
    while let Some(found) = lower_haystack[index..].find(&lower_from) {
        let start = index + found;
        let end = start + from.len();
        let preceded_by_word = start > 0 && bytes[start - 1].is_ascii_alphanumeric();
        let followed_by_word = end < bytes.len() && bytes[end].is_ascii_alphanumeric();
        if preceded_by_word || followed_by_word {
            result.push_str(&haystack[index..end]);
        } else {
            result.push_str(&haystack[index..start]);
            result.push_str(&spelling_with_source_case(&haystack[start..end], to));
        }
        index = end;
    }
    result.push_str(&haystack[index..]);
    result
}

fn spelling_with_source_case(source: &str, replacement: &str) -> String {
    if source
        .chars()
        .any(|character| character.is_ascii_alphabetic())
        && source
            .chars()
            .all(|character| !character.is_ascii_alphabetic() || character.is_ascii_uppercase())
    {
        return replacement.to_ascii_uppercase();
    }
    let Some(first) = source.chars().next() else {
        return replacement.to_string();
    };
    if first.is_ascii_uppercase() {
        let mut chars = replacement.chars();
        let Some(head) = chars.next() else {
            return replacement.to_string();
        };
        format!("{}{}", head.to_ascii_uppercase(), chars.as_str())
    } else {
        replacement.to_string()
    }
}

pub fn apply_locale_english_spelling(text: &str) -> String {
    if prefers_british_english() {
        apply_uk_spellings(text)
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_uk_spellings, identifier_is_british, spelling_with_source_case};

    #[test]
    fn converts_summarize() {
        assert_eq!(apply_uk_spellings("Please summarize this."), "Please summarise this.");
        assert_eq!(apply_uk_spellings("Summarize this."), "Summarise this.");
        assert_eq!(apply_uk_spellings("SUMMARIZE THIS."), "SUMMARISE THIS.");
    }

    #[test]
    fn leaves_size_alone() {
        assert_eq!(apply_uk_spellings("Check the size."), "Check the size.");
    }

    #[test]
    fn recognises_british_tags() {
        assert!(identifier_is_british("en_GB.UTF-8"));
        assert!(identifier_is_british("en-GB"));
        assert!(!identifier_is_british("en-US"));
    }

    #[test]
    fn matches_title_case() {
        assert_eq!(spelling_with_source_case("Color", "colour"), "Colour");
    }
}
