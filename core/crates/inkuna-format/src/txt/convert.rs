//! Plain-text to EPUB conversion.

use std::io::Read;
use std::path::Path;

use super::chapters::{detect_sections, is_cjk_dominant, DetectedSection};
use super::charset::decode_text;
use super::paragraphs::{
    floor_char_boundary, paragraph_blocks, render_chapter, render_volume_break, split_blocks,
};
use crate::EpubWriter;
use inkuna_content::MAX_TOTAL_TEXT_BYTES;
use crate::FormatError;

const FALLBACK_CHUNK_BYTES: usize = 10 * 1024;
const MAX_TXT_CHAPTERS: usize = 9_990;
// UTF-16 ASCII can occupy twice as many source bytes as decoded UTF-8.
const MAX_TXT_SOURCE_BYTES: usize = MAX_TOTAL_TEXT_BYTES * 2 + 3;

/// What [`convert_to_epub`] reports back about a finished conversion.
#[derive(Debug, PartialEq, Eq)]
pub struct TxtConversion {
    /// The `encoding_rs` canonical name of the detected source charset
    /// (e.g. `UTF-8`, `GBK`, `UTF-16LE`). Persisted verbatim in the
    /// `publications.text_encoding` column, so it is a stored schema
    /// value — never rename or re-case these strings.
    pub encoding: String,
}

enum OutputSection {
    Volume(String),
    Chapter { title: String, xhtml: String },
}

struct PendingChapter {
    volume: Option<String>,
    title: String,
    xhtml: String,
    count: usize,
}

/// Converts the plain-text file at `src` into an EPUB at `dst`, detecting
/// charset, chapters, and paragraph shape. The returned
/// [`TxtConversion::encoding`] is the `encoding_rs` canonical name the
/// importer persists in `publications.text_encoding`.
pub fn convert_to_epub(
    src: &Path,
    dst: &Path,
    fallback_title: &str,
) -> Result<TxtConversion, FormatError> {
    let mut bytes = Vec::new();
    std::fs::File::open(src)?
        .take(MAX_TXT_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_TXT_SOURCE_BYTES {
        return Err(FormatError::InvalidPublication(format!(
            "plain-text source exceeds {MAX_TXT_SOURCE_BYTES} bytes"
        )));
    }
    let decoded = decode_text(&bytes);
    if decoded.text.len() > MAX_TOTAL_TEXT_BYTES {
        return Err(FormatError::InvalidPublication(format!(
            "decoded plain text exceeds {MAX_TOTAL_TEXT_BYTES} bytes"
        )));
    }
    if decoded.text.trim().is_empty() {
        return Err(FormatError::InvalidPublication(
            "plain-text publication is empty".into(),
        ));
    }

    let language = language_for(&decoded.encoding, &decoded.text);
    let detected = match detect_sections(&decoded.text)? {
        sections if sections.is_empty() => fallback_sections(&decoded.text),
        sections => sections,
    };
    let mut output = expand_sections(detected);
    coalesce_chapters(&mut output);
    let output_text_budget: usize = output
        .iter()
        .filter_map(|section| match section {
            OutputSection::Volume(_) => None,
            OutputSection::Chapter { xhtml, .. } => Some(xhtml.len()),
        })
        .sum();
    if output_text_budget > MAX_TOTAL_TEXT_BYTES {
        return Err(FormatError::InvalidPublication(format!(
            "converted plain text exceeds {MAX_TOTAL_TEXT_BYTES} bytes"
        )));
    }

    let title = nfc(fallback_title);
    let mut writer = EpubWriter::new(&title);
    writer.language(language);
    for section in output {
        match section {
            OutputSection::Volume(title) => writer.begin_volume(&title),
            OutputSection::Chapter { title, xhtml } => writer.add_chapter(&title, &xhtml),
        }
    }
    writer.finish(dst)?;
    Ok(TxtConversion {
        encoding: decoded.encoding,
    })
}

fn expand_sections(sections: Vec<DetectedSection>) -> Vec<OutputSection> {
    let mut output = Vec::new();
    for section in sections {
        match section {
            DetectedSection::Volume { title } => output.push(OutputSection::Volume(title)),
            DetectedSection::Chapter { title, body } => {
                let blocks = paragraph_blocks(&body);
                for (index, chunk) in split_blocks(blocks).into_iter().enumerate() {
                    let chapter_title = if index == 0 {
                        title.clone()
                    } else {
                        format!("{title} ({})", index + 1)
                    };
                    output.push(OutputSection::Chapter {
                        xhtml: render_chapter(&chapter_title, &chunk),
                        title: chapter_title,
                    });
                }
            }
        }
    }
    output
}

fn fallback_sections(text: &str) -> Vec<DetectedSection> {
    let cjk = is_cjk_dominant(text);
    split_near_paragraphs(text, FALLBACK_CHUNK_BYTES)
        .into_iter()
        .enumerate()
        .map(|(index, body)| DetectedSection::Chapter {
            title: if cjk {
                format!("第{}部分", index + 1)
            } else {
                format!("Part {}", index + 1)
            },
            body: body.to_string(),
        })
        .collect()
}

fn split_near_paragraphs(mut text: &str, target: usize) -> Vec<&str> {
    let mut chunks = Vec::new();
    while text.len() > target {
        let target = floor_char_boundary(text, target);
        let before = text[..target].rfind("\n\n").map(|offset| offset + 2);
        // Keep each search window bounded: line-per-paragraph novels can
        // contain no blank-line delimiter at all.
        let window = floor_char_boundary(text, target.saturating_mul(2).min(text.len()));
        let after = text[target..window]
            .find("\n\n")
            .map(|offset| target + offset + 2);
        let split = match (before, after) {
            (Some(before), Some(after)) if target - before <= after - target => before,
            (_, Some(after)) => after,
            (Some(before), None) => before,
            (None, None) => target,
        };
        let (chunk, rest) = text.split_at(split);
        if !chunk.trim().is_empty() {
            chunks.push(chunk.trim_matches('\n'));
        }
        text = rest;
    }
    if !text.trim().is_empty() {
        chunks.push(text.trim_matches('\n'));
    }
    chunks
}

fn coalesce_chapters(sections: &mut Vec<OutputSection>) {
    let chapter_count = sections
        .iter()
        .filter(|section| matches!(section, OutputSection::Chapter { .. }))
        .count();
    if chapter_count <= MAX_TXT_CHAPTERS {
        return;
    }
    let factor = chapter_count.div_ceil(MAX_TXT_CHAPTERS);
    let mut merged = Vec::with_capacity(MAX_TXT_CHAPTERS);
    let mut active_volume = None;
    let mut pending: Option<PendingChapter> = None;
    for section in std::mem::take(sections) {
        match section {
            OutputSection::Volume(title) => active_volume = Some(title),
            OutputSection::Chapter { title, xhtml } => {
                let chapter_volume = active_volume.take();
                if let Some(entry) = &mut pending {
                    // A merge cannot represent two EPUB TOC volume groups.
                    // Preserve the later volume title visibly inside the
                    // merged chapter while retaining the first TOC group.
                    if let Some(volume) = chapter_volume {
                        entry.xhtml.push_str(&render_volume_break(&volume));
                    }
                    entry.xhtml.push_str(&xhtml);
                    entry.count += 1;
                } else {
                    pending = Some(PendingChapter {
                        volume: chapter_volume,
                        title,
                        xhtml,
                        count: 1,
                    });
                }
                if pending.as_ref().is_some_and(|entry| entry.count == factor) {
                    flush_pending(&mut merged, &mut pending);
                }
            }
        }
    }
    flush_pending(&mut merged, &mut pending);
    *sections = merged;
}

fn flush_pending(output: &mut Vec<OutputSection>, pending: &mut Option<PendingChapter>) {
    if let Some(entry) = pending.take() {
        if let Some(volume) = entry.volume {
            output.push(OutputSection::Volume(volume));
        }
        output.push(OutputSection::Chapter {
            title: entry.title,
            xhtml: entry.xhtml,
        });
    }
}

fn language_for(encoding: &str, text: &str) -> &'static str {
    match encoding {
        "Big5" | "GBK" => "zh",
        "Shift_JIS" | "EUC-JP" => "ja",
        "EUC-KR" => "ko",
        "UTF-8" | "UTF-16LE" | "UTF-16BE" => language_from_text(text),
        _ => "und",
    }
}

fn language_from_text(text: &str) -> &'static str {
    let sample: String = text.chars().take(4 * 1024).collect();
    if sample.chars().any(is_kana) {
        "ja"
    } else if sample.chars().any(is_hangul) {
        "ko"
    } else if sample.chars().filter(|ch| is_han(*ch)).count()
        > sample
            .chars()
            .filter(|ch| ch.is_alphabetic() && !is_han(*ch))
            .count()
    {
        "zh"
    } else {
        "und"
    }
}

fn is_han(ch: char) -> bool {
    matches!(ch as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F)
}

fn is_kana(ch: char) -> bool {
    matches!(ch as u32, 0x3040..=0x30FF)
}

fn is_hangul(ch: char) -> bool {
    matches!(ch as u32, 0xAC00..=0xD7AF)
}

fn nfc(text: &str) -> String {
    icu_normalizer::ComposingNormalizer::new_nfc()
        .normalize(text)
        .into_owned()
}

#[cfg(test)]
#[path = "convert_tests.rs"]
mod tests;
