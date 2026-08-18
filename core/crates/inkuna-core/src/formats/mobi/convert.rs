//! Classic MOBI6 conversion into the core's normalized EPUB 3 format.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use encoding_rs::WINDOWS_1252;

use super::markup;
use super::scan::{scan_markup, Scan};
use super::MobiBook;
use crate::formats::epub::EpubWriter;
use crate::CoreError;

const TARGET_CHUNK_BYTES: usize = 256 * 1024;
// XML escaping can expand one source byte sixfold. Keeping merged raw
// chunks under 768 KiB leaves room for injected anchors and XHTML wrapping
// below the writer's hard 8 MiB spine-entry cap.
const MAX_RAW_CHUNK_BYTES: usize = 768 * 1024;
const MAX_CHAPTERS: usize = 9_990;
const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
const MAX_TOTAL_IMAGE_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy)]
struct Chunk {
    start: usize,
    end: usize,
}

struct Edit {
    start: usize,
    end: usize,
    text: String,
    anchor: bool,
}

struct ImageBudget {
    used: usize,
    limit: usize,
}

impl ImageBudget {
    fn new(limit: usize) -> Self {
        Self { used: 0, limit }
    }

    fn reserve(&mut self, bytes: usize) -> bool {
        let Some(total) = self.used.checked_add(bytes) else {
            return false;
        };
        if total > self.limit {
            return false;
        }
        self.used = total;
        true
    }
}

pub(crate) fn convert_to_epub(
    src: &Path,
    dst: &Path,
    fallback_title: &str,
) -> Result<(), CoreError> {
    let book = MobiBook::open(src)?;
    if book.is_kf8() {
        return super::convert8::convert_to_epub(&book, dst, fallback_title);
    }
    let text = book.text()?;
    if text.is_empty() {
        return Err(invalid("MOBI6 publication has no text"));
    }
    let scan = scan_markup(&text)?;
    let chunks = chunks(&text, &scan, book.encoding());
    if chunks.is_empty() {
        return Err(invalid("MOBI6 publication has no convertible content"));
    }

    let title = super::sanitize::filter_xml_chars(&match book.title() {
        title if !title.trim().is_empty() => title,
        _ => fallback_title.to_string(),
    });
    if title.trim().is_empty() {
        return Err(invalid("untitled"));
    }
    let language = book.language().unwrap_or_else(|| "und".into());
    let mut writer = EpubWriter::new(&title);
    for author in book.authors() {
        writer.author(&super::sanitize::filter_xml_chars(&author));
    }
    writer.language(&language);
    writer.stylesheet(".mobi-center { text-align: center; }");
    if book
        .cover_len()
        .is_some_and(|length| length <= MAX_IMAGE_BYTES)
    {
        if let Some(cover) = book.cover() {
            if let Some((mime, _)) = image_type(cover) {
                writer.set_cover(cover.to_vec(), mime);
            }
        }
    }

    let target_chapters = target_chapters(&scan.targets, &chunks);
    let mut images = HashMap::<u32, Option<String>>::new();
    let mut image_budget = ImageBudget::new(MAX_TOTAL_IMAGE_BYTES);
    let mut meaningful = 0usize;
    for (chunk_index, chunk) in chunks.iter().enumerate() {
        let bytes = edited_chunk(&text, *chunk, chunk_index, &scan, &target_chapters);
        let decoded = decode(&bytes, book.encoding());
        let normalized = markup::normalize(&decoded, |index| {
            if let Some(href) = images.get(&index) {
                return href.clone();
            }
            let href = (|| {
                let length = book.image_len(index)?;
                if length > MAX_IMAGE_BYTES || !image_budget.reserve(length) {
                    return None;
                }
                let bytes = book.image(index)?;
                let (mime, extension) = image_type(bytes)?;
                Some(writer.add_image(&format!("img{index:05}.{extension}"), bytes.to_vec(), mime))
            })();
            images.insert(index, href.clone());
            href
        });
        if !normalized.xhtml.trim().is_empty() {
            meaningful += 1;
        }
        let chapter_number = chunk_index + 1;
        let chapter_title = normalized.heading.unwrap_or_else(|| {
            if language.starts_with("zh") {
                format!("第{chapter_number}章")
            } else {
                format!("Chapter {chapter_number}")
            }
        });
        writer.add_chapter(&chapter_title, &normalized.xhtml);
    }
    if meaningful == 0 {
        return Err(invalid("MOBI6 publication has no convertible content"));
    }
    writer.finish(dst)
}

fn decode(bytes: &[u8], encoding: u32) -> String {
    if !is_utf8(encoding) {
        WINDOWS_1252
            .decode_without_bom_handling(bytes)
            .0
            .into_owned()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn is_utf8(encoding: u32) -> bool {
    encoding != 1252
}

pub(super) fn image_type(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(("image/jpeg", "jpg"))
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("image/png", "png"))
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(("image/gif", "gif"))
    } else if bytes.starts_with(b"BM") {
        Some(("image/bmp", "bmp"))
    } else {
        None
    }
}

fn chunks(bytes: &[u8], scan: &Scan, encoding: u32) -> Vec<Chunk> {
    let mut hard = scan.pagebreaks.clone();
    hard.push(bytes.len());
    hard.sort_unstable();
    hard.dedup();
    let mut result = Vec::new();
    let mut start = 0;
    for end in hard {
        while end.saturating_sub(start) > TARGET_CHUNK_BYTES {
            let target = start + TARGET_CHUNK_BYTES;
            let split = scan.block_starts.partition_point(|point| *point < target);
            let candidate =
                scan.block_starts.get(split).copied().filter(|point| {
                    *point < end && point.saturating_sub(start) <= MAX_RAW_CHUNK_BYTES
                });
            let tag = scan
                .tag_starts
                .get(scan.tag_starts.partition_point(|point| *point < target))
                .copied()
                .filter(|point| *point < end && point.saturating_sub(start) <= MAX_RAW_CHUNK_BYTES);
            let split = candidate
                .or(tag)
                .unwrap_or_else(|| safe_split(bytes, target, end, encoding));
            if split <= start || split >= end {
                break;
            }
            result.push(Chunk { start, end: split });
            start = split;
        }
        if end > start {
            result.push(Chunk { start, end });
        }
        start = end;
    }
    result.retain(|chunk| {
        scan.targets.range(chunk.start..chunk.end).next().is_some()
            || !pagebreak_only(&bytes[chunk.start..chunk.end])
    });
    coalesce(&mut result);
    result
}

fn pagebreak_only(mut bytes: &[u8]) -> bool {
    loop {
        while bytes.first().is_some_and(u8::is_ascii_whitespace) {
            bytes = &bytes[1..];
        }
        if bytes.is_empty() {
            return true;
        }
        if !bytes.starts_with(b"<") {
            return false;
        }
        let Some(end) = bytes.iter().position(|byte| *byte == b'>') else {
            return false;
        };
        let name = bytes[1..end]
            .iter()
            .take_while(|byte| !byte.is_ascii_whitespace() && **byte != b'/')
            .map(u8::to_ascii_lowercase)
            .collect::<Vec<_>>();
        if name != b"mbp:pagebreak" {
            return false;
        }
        bytes = &bytes[end + 1..];
    }
}

fn safe_split(bytes: &[u8], mut target: usize, end: usize, encoding: u32) -> usize {
    target = target.min(end);
    if is_utf8(encoding) {
        while target < end && bytes.get(target).is_some_and(|byte| byte & 0xc0 == 0x80) {
            target += 1;
        }
    }
    target
}

fn coalesce(chunks: &mut Vec<Chunk>) {
    if chunks.len() <= MAX_CHAPTERS {
        return;
    }
    let factor = chunks.len().div_ceil(MAX_CHAPTERS);
    let merged = merge_chunks(chunks, factor);
    *chunks = if merged.len() <= MAX_CHAPTERS {
        merged
    } else {
        // A cluster of already-large chunks may prevent even grouping.
        // Greedy byte packing is the minimal bounded fallback.
        merge_chunks(&merged, usize::MAX)
    };
}

fn merge_chunks(chunks: &[Chunk], max_per_group: usize) -> Vec<Chunk> {
    let mut merged = Vec::<Chunk>::new();
    let mut count = 0usize;
    for chunk in chunks {
        let can_merge = merged.last().is_some_and(|current| {
            count < max_per_group && chunk.end - current.start <= MAX_RAW_CHUNK_BYTES
        });
        if can_merge {
            if let Some(current) = merged.last_mut() {
                current.end = chunk.end;
            }
            count += 1;
        } else {
            merged.push(*chunk);
            count = 1;
        }
    }
    merged
}

fn target_chapters(targets: &BTreeSet<usize>, chunks: &[Chunk]) -> HashMap<usize, usize> {
    targets
        .iter()
        .filter_map(|target| {
            let chapter = chunks.partition_point(|chunk| chunk.end <= *target);
            chunks
                .get(chapter)
                .filter(|chunk| *target >= chunk.start && *target < chunk.end)
                .map(|_| (*target, chapter))
        })
        .collect()
}

fn edited_chunk(
    bytes: &[u8],
    chunk: Chunk,
    chapter: usize,
    scan: &Scan,
    target_chapters: &HashMap<usize, usize>,
) -> Vec<u8> {
    let mut edits = Vec::new();
    for target in scan.targets.range(chunk.start..chunk.end) {
        let before = scan.tag_starts.partition_point(|start| *start <= *target);
        let position = scan.tag_starts[..before]
            .iter()
            .rev()
            .copied()
            .find(|position| *position >= chunk.start)
            .unwrap_or(chunk.start);
        edits.push(Edit {
            start: position,
            end: position,
            text: format!("<a id=\"fp{target}\"></a>"),
            anchor: true,
        });
    }
    for link in scan
        .links
        .iter()
        .filter(|link| link.start >= chunk.start && link.start < chunk.end)
    {
        let href = target_chapters.get(&link.target).map(|target_chapter| {
            if *target_chapter == chapter {
                format!("#fp{}", link.target)
            } else {
                format!("ch{:05}.xhtml#fp{}", target_chapter + 1, link.target)
            }
        });
        let mut tag = String::from("<a");
        if let Some(href) = href {
            tag.push_str(&format!(" href=\"{href}\""));
        }
        if let Some(id) = &link.id {
            tag.push_str(&format!(" id=\"{id}\""));
        }
        tag.push('>');
        edits.push(Edit {
            start: link.start,
            end: link.end,
            text: tag,
            anchor: false,
        });
    }
    edits.sort_by_key(|edit| (edit.start, !edit.anchor));
    let mut output = Vec::with_capacity(chunk.end - chunk.start + edits.len() * 24);
    let mut cursor = chunk.start;
    for edit in edits {
        if edit.start < cursor {
            continue;
        }
        output.extend_from_slice(&bytes[cursor..edit.start]);
        output.extend_from_slice(edit.text.as_bytes());
        cursor = edit.end;
    }
    output.extend_from_slice(&bytes[cursor..chunk.end]);
    output
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidPublication(message.into())
}

#[cfg(test)]
#[path = "convert_tests.rs"]
mod tests;
