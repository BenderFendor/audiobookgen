use crate::model::PronunciationRule;
use regex::{Captures, Regex};
use std::sync::LazyLock;

pub const NORMALIZATION_VERSION: &str = "en-v1";

static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("valid regex"));
static MONEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$([0-9][0-9,]*)(?:\.([0-9]{2}))?").expect("valid regex"));
static PERCENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b([0-9]+(?:\.[0-9]+)?)%").expect("valid regex"));
static TIME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b([0-9]{1,2}):([0-9]{2})\b").expect("valid regex"));
static ROMAN_CHAPTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(chapter|book|volume)\s+([ivxlcdm]+)\b").expect("valid regex"));

pub fn normalize_for_speech(input: &str, rules: &[PronunciationRule]) -> String {
    let mut text = input
        .replace('\u{00a0}', " ")
        .replace('…', "...")
        .replace('—', " — ")
        .replace('–', "-");
    text = expand_common_abbreviations(&text);
    text = MONEY.replace_all(&text, |caps: &Captures| money(caps)).into_owned();
    text = PERCENT.replace_all(&text, "$1 percent").into_owned();
    text = TIME.replace_all(&text, |caps: &Captures| {
        let hour = caps[1].parse::<u32>().unwrap_or(0);
        let minute = caps[2].parse::<u32>().unwrap_or(0);
        if minute == 0 { format!("{hour} o'clock") } else if minute < 10 { format!("{hour} oh {minute}") } else { format!("{hour} {minute}") }
    }).into_owned();
    text = ROMAN_CHAPTER.replace_all(&text, |caps: &Captures| {
        roman_to_u32(&caps[2]).map(|value| format!("{} {}", &caps[1], value)).unwrap_or_else(|| caps[0].to_owned())
    }).into_owned();
    for rule in rules {
        if rule.pattern.is_empty() { continue; }
        if rule.case_sensitive { text = text.replace(&rule.pattern, &rule.replacement); }
        else if let Ok(regex) = Regex::new(&format!("(?i){}", regex::escape(&rule.pattern))) { text = regex.replace_all(&text, rule.replacement.as_str()).into_owned(); }
    }
    WHITESPACE.replace_all(text.trim(), " ").into_owned()
}

fn expand_common_abbreviations(input: &str) -> String {
    let replacements = [
        (r"\bMr\.", "Mister"), (r"\bMrs\.", "Missus"), (r"\bMs\.", "Miz"),
        (r"\bDr\.", "Doctor"), (r"\bProf\.", "Professor"), (r"\bSt\.", "Saint"),
        (r"\bNo\.", "number"), (r"\be\.g\.", "for example"), (r"\bi\.e\.", "that is"),
    ];
    replacements.into_iter().fold(input.to_owned(), |text, (pattern, replacement)| {
        Regex::new(pattern).expect("valid abbreviation regex").replace_all(&text, replacement).into_owned()
    })
}

fn money(caps: &Captures) -> String {
    let dollars = caps[1].replace(',', "");
    let amount = dollars.parse::<u64>().unwrap_or(0);
    let unit = if amount == 1 { "dollar" } else { "dollars" };
    match caps.get(2).map(|value| value.as_str()).filter(|value| value != &"00") {
        Some(cents) => format!("{amount} {unit} and {} cents", cents.parse::<u32>().unwrap_or(0)),
        None => format!("{amount} {unit}"),
    }
}

fn roman_to_u32(input: &str) -> Option<u32> {
    let mut total = 0i32;
    let mut previous = 0i32;
    for character in input.to_ascii_uppercase().chars().rev() {
        let value = match character { 'I' => 1, 'V' => 5, 'X' => 10, 'L' => 50, 'C' => 100, 'D' => 500, 'M' => 1000, _ => return None };
        if value < previous { total -= value; } else { total += value; previous = value; }
    }
    (total > 0).then_some(total as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn expands_bookish_text() {
        let output = normalize_for_speech("Dr. Reed paid $12.50 at 8:05 — a 25% rise.", &[]);
        assert_eq!(output, "Doctor Reed paid 12 dollars and 50 cents at 8 oh 5 — a 25 percent rise.");
    }
    #[test] fn expands_roman_chapters() { assert_eq!(normalize_for_speech("Chapter XIV", &[]), "Chapter 14"); }
}
