use crate::cache::segment_cache_key;
use crate::model::{Fragment, FragmentKind, FragmentLocator, NarrationBlock, NarrationProfile, PronunciationRule};
use crate::normalize::normalize_for_speech;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const PLANNER_VERSION: &str = "sentence-v1";
const MAX_FRAGMENT_CHARS: usize = 520;

pub fn plan_fragments(book_id: Uuid, blocks: Vec<NarrationBlock>, profile: &NarrationProfile, rules: &[PronunciationRule]) -> Vec<Fragment> {
    let mut fragments = Vec::new();
    for block in blocks {
        let sentences = if matches!(block.kind, FragmentKind::SceneBreak | FragmentKind::Table | FragmentKind::Caption | FragmentKind::Footnote | FragmentKind::Heading) {
            vec![block.text.clone()]
        } else { split_sentences(&block.text) };
        for sentence in sentences.into_iter().filter(|sentence| !sentence.trim().is_empty()) {
            let kind = if block.kind == FragmentKind::Sentence && looks_like_dialogue(&sentence) { FragmentKind::Dialogue } else { block.kind.clone() };
            let source_text = sentence.trim().to_owned();
            let spoken_text = normalize_for_speech(&source_text, rules);
            let source_text_hash = hex::encode(Sha256::digest(source_text.as_bytes()));
            let index = fragments.len();
            let pause_after_ms = pause_for(&source_text, &kind);
            let mut fragment = Fragment {
                id: Uuid::new_v4(), book_id, chapter_id: block.chapter_id, chapter_index: block.chapter_index, index,
                source_text, spoken_text, kind,
                locator: FragmentLocator { href: block.href.clone(), css_selector: Some(format!("#ag-block-{}", &source_text_hash[..12])), text_occurrence: block.occurrence, source_text_hash, cfi: None },
                pause_after_ms, cache_key: String::new(),
            };
            fragment.cache_key = segment_cache_key(&fragment, profile);
            fragments.push(fragment);
        }
    }
    fragments
}

pub fn split_sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut output = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;
    while index < chars.len() {
        let character = chars[index];
        let boundary = matches!(character, '.' | '!' | '?') && !is_abbreviation_boundary(&chars, index);
        if boundary {
            let mut end = index + 1;
            while end < chars.len() && matches!(chars[end], '"' | '\'' | '”' | '’' | ')' | ']') { end += 1; }
            if end == chars.len() || chars[end].is_whitespace() {
                push_chunked(&mut output, chars[start..end].iter().collect::<String>());
                start = end;
                while start < chars.len() && chars[start].is_whitespace() { start += 1; }
                index = start;
                continue;
            }
        }
        index += 1;
    }
    if start < chars.len() { push_chunked(&mut output, chars[start..].iter().collect::<String>()); }
    output
}

fn push_chunked(output: &mut Vec<String>, sentence: String) {
    let sentence = sentence.trim();
    if sentence.len() <= MAX_FRAGMENT_CHARS { if !sentence.is_empty() { output.push(sentence.to_owned()); } return; }
    let mut remaining = sentence;
    while remaining.len() > MAX_FRAGMENT_CHARS {
        let safe = remaining.char_indices().take_while(|(index, _)| *index <= MAX_FRAGMENT_CHARS).filter(|(_, character)| matches!(character, ',' | ';' | ':' | '—')).map(|(index, _)| index).last()
            .or_else(|| remaining.char_indices().take_while(|(index, _)| *index <= MAX_FRAGMENT_CHARS).filter(|(_, character)| character.is_whitespace()).map(|(index, _)| index).last())
            .unwrap_or_else(|| remaining.char_indices().nth(MAX_FRAGMENT_CHARS / 2).map(|(index, _)| index).unwrap_or(remaining.len()));
        let (head, tail) = remaining.split_at(safe);
        output.push(head.trim().to_owned());
        remaining = tail.trim();
    }
    if !remaining.is_empty() { output.push(remaining.to_owned()); }
}

fn is_abbreviation_boundary(chars: &[char], index: usize) -> bool {
    if chars[index] != '.' { return false; }
    let before: String = chars[..index].iter().rev().take_while(|character| character.is_ascii_alphabetic()).collect::<Vec<_>>().into_iter().rev().collect();
    let common = ["Mr", "Mrs", "Ms", "Dr", "Prof", "St", "No", "Jr", "Sr", "vs", "etc", "e", "i"];
    if common.iter().any(|value| before.eq_ignore_ascii_case(value)) { return true; }
    let previous = index.checked_sub(1).and_then(|value| chars.get(value));
    let next = chars.get(index + 1);
    matches!((previous, next), (Some(a), Some(b)) if a.is_ascii_digit() && b.is_ascii_digit()) || before.len() == 1
}

fn looks_like_dialogue(text: &str) -> bool { matches!(text.trim_start().chars().next(), Some('\"' | '“' | '\'' | '‘')) }
fn pause_for(text: &str, kind: &FragmentKind) -> u32 {
    match kind {
        FragmentKind::Heading => 950,
        FragmentKind::SceneBreak => 1400,
        FragmentKind::Table => 700,
        FragmentKind::Caption | FragmentKind::Footnote => 520,
        _ if text.ends_with('?') || text.ends_with('!') => 430,
        _ => 340,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn keeps_abbreviations_together() { assert_eq!(split_sentences("Dr. Reed left. Then she returned."), vec!["Dr. Reed left.", "Then she returned."]); }
    #[test] fn separates_dialogue() { assert_eq!(split_sentences("\"Go.\" He stayed."), vec!["\"Go.\"", "He stayed."]); }
}
