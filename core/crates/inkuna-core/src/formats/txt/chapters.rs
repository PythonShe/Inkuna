//! Legado-style chapter-rule competition and structural splitting.

use std::collections::BTreeMap;

use regex::{Captures, Regex};

use crate::CoreError;

const SAMPLE_BYTES: usize = 1024 * 1024;
const NUMERAL: &str = r"[\d０-９〇零一二两三四五六七八九十百千万壹贰叁肆伍陆柒捌玖拾佰仟]";

#[derive(Debug, PartialEq)]
pub(super) enum DetectedSection {
    Volume { title: String },
    Chapter { title: String, body: String },
}

#[derive(Clone, Copy)]
enum Rule {
    Chinese,
    Western,
    Numeric,
    Bracketed,
}

#[derive(Clone)]
struct Heading {
    start: usize,
    end: usize,
    title: String,
    volume: bool,
}

struct ChapterRules {
    bracketed: Regex,
    chinese: Regex,
    numeric: Regex,
    volume: Regex,
    western: Regex,
}

impl ChapterRules {
    fn compile() -> Result<Self, CoreError> {
        Ok(Self {
            bracketed: compile_regex(&format!(
                r"(?m)^[ \t　]{{0,4}}[【〔〖「『〈［\[](?:第|[Cc]hapter)\s{{0,4}}{NUMERAL}{{1,10}}\s{{0,4}}[章节回话].{{0,20}}$"
            ))?,
            chinese: compile_regex(&format!(
                r"(?m)^[ \t　]{{0,4}}(?:(?P<special>序章|楔子|正文|终章|后记|尾声|番外)|第\s{{0,4}}{NUMERAL}+?\s{{0,4}}(?P<unit>章|节|卷|集|部|篇|回|话))(?P<tail>.{{0,30}})$"
            ))?,
            numeric: compile_regex(r"(?m)^[ \t　]{0,4}\d{1,5}[:：,.，、_—\-].{1,30}$")?,
            volume: compile_regex(&format!(
                r"(?m)^[ \t　]{{0,4}}(?:第\s{{0,4}}{NUMERAL}+?\s{{0,4}}卷|卷{NUMERAL}{{1,8}}|[上中下][部篇卷]).{{0,30}}$"
            ))?,
            western: compile_regex(
                r"(?m)^[ \t]{0,3}(?:(?:(?:CHAPTER|Chapter|PART|Part|BOOK|Book)\s+(?:\d{1,4}|[IVXLCivxlc]{1,8})\b)|(?:(?:Prologue|Epilogue|Interlude|Foreword|Preface|Afterword|Introduction)\b)).{0,40}$",
            )?,
        })
    }
}

/// Splits `text` into volume/chapter sections by rule competition: the
/// four heading rules (Chinese, Western, numeric, bracketed) are scored
/// over the first 1 MiB and the highest-scoring rule wins — but only if
/// it matched at least 3 headings there; otherwise the result is empty
/// and the caller falls back to size-based parts. Text before the first
/// heading becomes a "前言"/"Preface" chapter; volume headings and
/// body-less headings become volume entries.
pub(super) fn detect_sections(text: &str) -> Result<Vec<DetectedSection>, CoreError> {
    let rules = ChapterRules::compile()?;
    let sample = prefix_at_char_boundary(text, SAMPLE_BYTES);
    let mut winner = None;
    let mut winning_count = 0;
    for rule in [Rule::Chinese, Rule::Western, Rule::Numeric, Rule::Bracketed] {
        let count = rule_matches(&rules, rule, sample).len();
        if count > winning_count {
            winner = Some(rule);
            winning_count = count;
        }
    }
    if winning_count < 3 {
        return Ok(Vec::new());
    }

    let Some(winner) = winner else {
        return Ok(Vec::new());
    };
    let mut by_start: BTreeMap<usize, Heading> = rule_matches(&rules, winner, text)
        .into_iter()
        .map(|heading| (heading.start, heading))
        .collect();
    for volume in volume_matches(&rules, text) {
        by_start
            .entry(volume.start)
            .and_modify(|heading| heading.volume = true)
            .or_insert(volume);
    }
    let headings: Vec<Heading> = by_start.into_values().collect();
    if headings.is_empty() {
        return Ok(Vec::new());
    }

    let mut sections = Vec::new();
    let preface = text[..headings[0].start].trim();
    if !preface.is_empty() {
        sections.push(DetectedSection::Chapter {
            title: if is_cjk_dominant(text) {
                "前言".into()
            } else {
                "Preface".into()
            },
            body: preface.into(),
        });
    }
    for (index, heading) in headings.iter().enumerate() {
        let body_end = headings
            .get(index + 1)
            .map_or(text.len(), |next| next.start);
        let body = text[heading.end..body_end].trim_matches('\n');
        if heading.volume || body.trim().is_empty() {
            sections.push(DetectedSection::Volume {
                title: heading.title.clone(),
            });
        } else {
            sections.push(DetectedSection::Chapter {
                title: heading.title.clone(),
                body: body.into(),
            });
        }
    }
    Ok(sections)
}

pub(super) fn is_cjk_dominant(text: &str) -> bool {
    let (cjk, latin) = text.chars().fold((0usize, 0usize), |(cjk, latin), ch| {
        if is_cjk(ch) {
            (cjk + 1, latin)
        } else if ch.is_alphabetic() {
            (cjk, latin + 1)
        } else {
            (cjk, latin)
        }
    });
    cjk > latin
}

fn rule_matches(rules: &ChapterRules, rule: Rule, text: &str) -> Vec<Heading> {
    match rule {
        Rule::Chinese => rules
            .chinese
            .captures_iter(text)
            .filter(valid_chinese_match)
            .filter_map(heading_from_captures)
            .collect(),
        Rule::Western => plain_matches(&rules.western, text),
        Rule::Numeric => plain_matches(&rules.numeric, text),
        Rule::Bracketed => plain_matches(&rules.bracketed, text),
    }
}

fn volume_matches(rules: &ChapterRules, text: &str) -> Vec<Heading> {
    plain_matches(&rules.volume, text)
        .into_iter()
        .map(|mut heading| {
            heading.volume = true;
            heading
        })
        .collect()
}

fn plain_matches(regex: &Regex, text: &str) -> Vec<Heading> {
    regex
        .find_iter(text)
        .map(|found| Heading {
            start: found.start(),
            end: found.end(),
            title: trim_heading(found.as_str()),
            volume: false,
        })
        .collect()
}

fn heading_from_captures(captures: Captures<'_>) -> Option<Heading> {
    let found = captures.get(0)?;
    Some(Heading {
        start: found.start(),
        end: found.end(),
        title: trim_heading(found.as_str()),
        volume: false,
    })
}

fn valid_chinese_match(captures: &Captures<'_>) -> bool {
    let tail = captures.name("tail").map_or("", |value| value.as_str());
    let next = tail.chars().next();
    if captures.name("special").map(|value| value.as_str()) == Some("正文") {
        return !matches!(next, Some('完' | '结'));
    }
    match captures.name("unit").map(|value| value.as_str()) {
        Some("节") => !matches!(next, Some('课')),
        Some("集") => !matches!(next, Some('合' | '和')),
        Some("部") => !matches!(next, Some('分' | '赛' | '游')),
        Some("篇") => !matches!(next, Some('张')),
        Some("回") => !matches!(next, Some('合' | '来' | '事' | '去')),
        _ => true,
    }
}

fn trim_heading(heading: &str) -> String {
    heading
        .trim_start_matches([' ', '\t', '\u{3000}'])
        .trim_end()
        .to_string()
}

fn prefix_at_char_boundary(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn compile_regex(pattern: &str) -> Result<Regex, CoreError> {
    Regex::new(pattern).map_err(|error| {
        CoreError::InvalidPublication(format!("invalid TXT chapter rule: {error}"))
    })
}

pub(super) fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF |
        0x3040..=0x30FF | 0xAC00..=0xD7AF | 0x20000..=0x2FA1F)
}

#[cfg(test)]
#[path = "chapters_tests.rs"]
mod tests;
