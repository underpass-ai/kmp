/// Words a fact key skips: they name nothing.
const STOP: [&str; 42] = [
    "a", "an", "the", "of", "to", "in", "on", "for", "and", "or", "is", "are", "was", "were", "be",
    "we", "it", "its", "at", "by", "with", "from", "as", "that", "this", "el", "la", "los", "las",
    "de", "del", "en", "y", "o", "un", "una", "que", "se", "por", "con", "para", "al",
];
const KEY_WORDS: usize = 4;
const KEY_CHARS: usize = 40;

/// A stable key per fact, in the given order, made of its first informative
/// words (lowercase ASCII letters and digits, at most four words and 40
/// characters), with `_2`, `_3` ... on a repeat. TypeSafe sends option names
/// to the model and reads positional ones as nothing to go on: a key that
/// says what the fact is about gives the choice something to read.
pub(crate) fn fact_keys<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen = std::collections::BTreeMap::<String, usize>::new();
    texts
        .into_iter()
        .map(|text| {
            let stem = stem(text);
            let count = seen.entry(stem.clone()).or_insert(0);
            *count += 1;
            if *count == 1 {
                stem
            } else {
                format!("{stem}_{count}")
            }
        })
        .collect()
}

fn stem(text: &str) -> String {
    let lower = text.to_lowercase();
    let words = lower
        .split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        .filter(|word| !word.is_empty() && !STOP.contains(word))
        .take(KEY_WORDS)
        .collect::<Vec<_>>()
        .join("_");
    let cut = words.chars().take(KEY_CHARS).collect::<String>();
    let trimmed = cut.trim_matches('_');
    if trimmed.is_empty() {
        "fact".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::fact_keys;

    #[test]
    fn keys_are_the_first_informative_words_and_repeats_are_numbered() {
        assert_eq!(
            fact_keys([
                "CHG-200011: we chose to pause",
                "The large outline mentions peqi",
                "CHG-200011: we chose to pause again",
                "¿Qué?",
                "The of and",
            ]),
            [
                "chg_200011_chose_pause",
                "large_outline_mentions_peqi",
                "chg_200011_chose_pause_2",
                "qu",
                "fact",
            ]
        );
    }

    #[test]
    fn accented_letters_separate_words_as_ascii_does() {
        assert_eq!(fact_keys(["Migración del clúster"]), ["migraci_n_cl_ster"]);
    }
}
