//! KF8/AZW3 conversion into the core's normalized EPUB 3 format.

use std::collections::HashMap;
use std::path::Path;

use super::kf8::{self, FragmentAnchor, Kf8Content};
use crate::formats::epub::EpubWriter;
use crate::formats::mobi::{image_type, markup, sanitize, MobiBook};
use crate::CoreError;

const BASE32: &[u8; 32] = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";
const TARGET_RAW_BYTES: usize = 512 * 1024;
const MAX_RAW_CHAPTER_BYTES: usize = 768 * 1024;
const MAX_CHAPTERS: usize = 9_990;
const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
const MAX_TOTAL_IMAGE_BYTES: usize = 128 * 1024 * 1024;

struct Part<'a> {
    file: usize,
    bytes: &'a [u8],
    anchors: Vec<FragmentAnchor>,
}

struct ImageBudget {
    used: usize,
}

impl ImageBudget {
    fn reserve(&mut self, bytes: usize) -> bool {
        let Some(total) = self.used.checked_add(bytes) else {
            return false;
        };
        if total > MAX_TOTAL_IMAGE_BYTES {
            return false;
        }
        self.used = total;
        true
    }
}

pub(crate) fn convert_to_epub(
    book: &MobiBook,
    dst: &Path,
    fallback_title: &str,
) -> Result<(), CoreError> {
    let content = kf8::read(book)?;
    if content.files.is_empty() {
        return Err(invalid("KF8 publication has no skeleton files"));
    }
    let title = sanitize::filter_xml_chars(&match book.title() {
        title if !title.trim().is_empty() => title,
        _ => fallback_title.to_string(),
    });
    if title.trim().is_empty() {
        return Err(invalid("untitled"));
    }
    let language = book.language().unwrap_or_else(|| "und".into());
    let mut writer = EpubWriter::new(&title);
    for author in book.authors() {
        writer.author(&sanitize::filter_xml_chars(&author));
    }
    writer.language(&language);
    writer.stylesheet(".mobi-center { text-align: center; }");
    append_css(&content, &mut writer);
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

    let parts = split_parts(&content);
    let groups = group_parts(parts);
    if groups.is_empty() || groups.len() > MAX_CHAPTERS {
        return Err(invalid("KF8 spine cannot be coalesced below 9990 chapters"));
    }
    let fragment_chapters = target_chapters(&groups);
    let mut images = HashMap::<u32, Option<String>>::new();
    let mut image_budget = ImageBudget { used: 0 };
    let mut file_seen = vec![false; content.files.len()];
    let mut meaningful = 0usize;

    for (chapter, group) in groups.iter().enumerate() {
        let first_file = group.first().map(|part| part.file).unwrap_or(0);
        let toc = content
            .toc
            .as_ref()
            .and_then(|entries| entries.iter().find(|entry| entry.file == first_file));
        if !file_seen[first_file] {
            if let Some(entry) = toc.filter(|entry| entry.depth == 1) {
                writer.begin_volume(&entry.label);
            }
            file_seen[first_file] = true;
        }
        let mut xhtml = String::new();
        let mut heading = None;
        for part in group {
            let anchored = insert_anchors(part.bytes, &part.anchors);
            let decoded = String::from_utf8_lossy(&anchored);
            let body = body_inner(&decoded);
            let rewritten = rewrite_kindle_uris(body, chapter, &fragment_chapters, |index| {
                image_href(book, &mut writer, &mut images, &mut image_budget, index)
            });
            let normalized = markup::normalize(&rewritten, |index| {
                image_href(book, &mut writer, &mut images, &mut image_budget, index)
            });
            if heading.is_none() {
                heading = normalized.heading;
            }
            xhtml.push_str(&normalized.xhtml);
        }
        if !xhtml.trim().is_empty() {
            meaningful += 1;
        }
        let number = chapter + 1;
        let chapter_title = toc
            .map(|entry| entry.label.clone())
            .or(heading)
            .unwrap_or_else(|| {
                if language.starts_with("zh") {
                    format!("第{number}章")
                } else {
                    format!("Chapter {number}")
                }
            });
        writer.add_chapter(&chapter_title, &xhtml);
    }
    if meaningful == 0 {
        return Err(invalid("KF8 publication has no convertible content"));
    }
    writer.finish(dst)
}

fn append_css(content: &Kf8Content, writer: &mut EpubWriter) {
    for flow in content.flows.iter().skip(1) {
        let Ok(text) = std::str::from_utf8(flow) else {
            continue;
        };
        let trimmed = text.trim_start();
        if trimmed.starts_with("<svg") || trimmed.starts_with("<?xml") {
            continue;
        }
        if text.contains('{') && text.contains('}') {
            let safe = sanitize_css(text);
            if !safe.trim().is_empty() {
                writer.stylesheet(&safe);
            }
        }
    }
}

pub(super) fn sanitize_css(input: &str) -> String {
    let mut css = input.to_string();
    while let Some(start) = find_ascii_case_insensitive(&css, "@import") {
        let end = css[start..]
            .find(';')
            .map_or(css.len(), |relative| start + relative + 1);
        css.replace_range(start..end, "");
    }
    let mut cursor = 0usize;
    while let Some(relative) = find_ascii_case_insensitive(&css[cursor..], "url(") {
        let start = cursor + relative;
        let Some(close_relative) = css[start + 4..].find(')') else {
            css.truncate(start);
            break;
        };
        let end = start + 4 + close_relative + 1;
        let value = css[start + 4..end - 1].trim().trim_matches(['\'', '"']);
        if is_remote_css_url(value) {
            css.replace_range(start..end, "none");
            cursor = start + 4;
        } else {
            cursor = end;
        }
    }
    css
}

fn is_remote_css_url(value: &str) -> bool {
    value.starts_with("//")
        || value
            .split_once(':')
            .is_some_and(|(scheme, _)| !scheme.eq_ignore_ascii_case("file"))
}

fn split_parts(content: &Kf8Content) -> Vec<Part<'_>> {
    let mut parts = Vec::new();
    for (file, assembled) in content.files.iter().enumerate() {
        let mut start = 0usize;
        while start < assembled.bytes.len() {
            let end = split_end(&assembled.bytes, start);
            let anchors = assembled
                .anchors
                .iter()
                .filter(|anchor| anchor.offset >= start && anchor.offset < end)
                .map(|anchor| FragmentAnchor {
                    fid: anchor.fid,
                    offset: anchor.offset - start,
                })
                .collect();
            parts.push(Part {
                file,
                bytes: &assembled.bytes[start..end],
                anchors,
            });
            start = end;
        }
        if assembled.bytes.is_empty() {
            parts.push(Part {
                file,
                bytes: &[],
                anchors: Vec::new(),
            });
        }
    }
    parts
}

fn split_end(bytes: &[u8], start: usize) -> usize {
    if bytes.len() - start <= MAX_RAW_CHAPTER_BYTES {
        return bytes.len();
    }
    let target = start + TARGET_RAW_BYTES;
    let limit = (start + MAX_RAW_CHAPTER_BYTES).min(bytes.len());
    let mut end = bytes[target..limit]
        .iter()
        .position(|byte| *byte == b'<')
        .map_or(target, |relative| target + relative);
    while end < bytes.len() && bytes[end] & 0xc0 == 0x80 {
        end += 1;
    }
    end.min(limit).max(start + 1)
}

fn group_parts(parts: Vec<Part<'_>>) -> Vec<Vec<Part<'_>>> {
    if parts.len() <= MAX_CHAPTERS {
        return parts.into_iter().map(|part| vec![part]).collect();
    }
    let mut groups = Vec::<Vec<Part<'_>>>::new();
    let mut used = 0usize;
    for part in parts {
        let can_append = groups.last().is_some()
            && used
                .checked_add(part.bytes.len())
                .is_some_and(|total| total <= MAX_RAW_CHAPTER_BYTES);
        if can_append {
            used += part.bytes.len();
            if let Some(group) = groups.last_mut() {
                group.push(part);
            }
        } else {
            used = part.bytes.len();
            groups.push(vec![part]);
        }
    }
    groups
}

fn target_chapters(groups: &[Vec<Part<'_>>]) -> HashMap<u32, usize> {
    let mut fragments = HashMap::new();
    for (chapter, group) in groups.iter().enumerate() {
        for part in group {
            for anchor in &part.anchors {
                fragments.insert(anchor.fid, chapter);
            }
        }
    }
    fragments
}

fn insert_anchors(bytes: &[u8], anchors: &[FragmentAnchor]) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len() + anchors.len() * 24);
    let mut cursor = 0usize;
    for anchor in anchors {
        if anchor.offset < cursor || anchor.offset > bytes.len() {
            continue;
        }
        output.extend_from_slice(&bytes[cursor..anchor.offset]);
        output.extend_from_slice(
            format!("<a id=\"kp{}\"></a>", encode_base32(anchor.fid)).as_bytes(),
        );
        cursor = anchor.offset;
    }
    output.extend_from_slice(&bytes[cursor..]);
    output
}

fn body_inner(input: &str) -> &str {
    let Some(start) = find_ascii_case_insensitive(input, "<body") else {
        return input;
    };
    let Some(open_end) = input[start..].find('>').map(|end| start + end + 1) else {
        return input;
    };
    let end = find_ascii_case_insensitive(&input[open_end..], "</body")
        .map_or(input.len(), |relative| open_end + relative);
    &input[open_end..end]
}

fn rewrite_kindle_uris(
    input: &str,
    chapter: usize,
    targets: &HashMap<u32, usize>,
    mut embed: impl FnMut(u32) -> Option<String>,
) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = find_ascii_case_insensitive(rest, "kindle:") {
        output.push_str(&rest[..start]);
        let tail = &rest[start..];
        let end = tail
            .find(|ch: char| ch.is_ascii_whitespace() || matches!(ch, '\'' | '"' | '<' | '>' | ')'))
            .unwrap_or(tail.len());
        let uri = &tail[..end];
        if let Some(fid) = parse_position(uri) {
            if let Some(target) = targets.get(&fid) {
                let anchor = encode_base32(fid);
                if *target == chapter {
                    output.push_str(&format!("#kp{anchor}"));
                } else {
                    output.push_str(&format!("ch{:05}.xhtml#kp{anchor}", target + 1));
                }
            }
        } else if let Some(index) = parse_resource(uri, "kindle:embed:") {
            if let Some(href) = embed(index) {
                output.push_str(&href);
            }
        }
        rest = &tail[end..];
    }
    output.push_str(rest);
    output
}

fn parse_position(uri: &str) -> Option<u32> {
    let value = strip_prefix_ascii_case(uri, "kindle:pos:fid:")?;
    let (fid, offset) = value.split_once(":off:")?;
    if fid.len() != 4 || offset.len() != 10 {
        return None;
    }
    let fid = u32::try_from(decode_base32(fid)?).ok()?;
    let _offset = decode_base32(offset)?;
    Some(fid)
}

fn parse_resource(uri: &str, prefix: &str) -> Option<u32> {
    let value = strip_prefix_ascii_case(uri, prefix)?;
    let id = value.split('?').next()?;
    (id.len() == 4)
        .then(|| u32::try_from(decode_base32(id)?).ok())
        .flatten()
}

fn image_href(
    book: &MobiBook,
    writer: &mut EpubWriter,
    cache: &mut HashMap<u32, Option<String>>,
    budget: &mut ImageBudget,
    index: u32,
) -> Option<String> {
    if let Some(href) = cache.get(&index) {
        return href.clone();
    }
    let href = (|| {
        let length = book.image_len(index)?;
        if length > MAX_IMAGE_BYTES || !budget.reserve(length) {
            return None;
        }
        let bytes = book.image(index)?;
        let (mime, extension) = image_type(bytes)?;
        Some(writer.add_image(
            &format!("kf8img{index:05}.{extension}"),
            bytes.to_vec(),
            mime,
        ))
    })();
    cache.insert(index, href.clone());
    href
}

fn encode_base32(mut value: u32) -> String {
    let mut encoded = [b'0'; 4];
    for byte in encoded.iter_mut().rev() {
        *byte = BASE32[(value & 31) as usize];
        value >>= 5;
    }
    String::from_utf8_lossy(&encoded).into_owned()
}

fn decode_base32(value: &str) -> Option<u64> {
    value.bytes().try_fold(0u64, |result, byte| {
        let digit = match byte.to_ascii_uppercase() {
            b'0'..=b'9' => u64::from(byte - b'0'),
            b'A'..=b'V' => u64::from(byte.to_ascii_uppercase() - b'A') + 10,
            _ => return None,
        };
        result.checked_mul(32)?.checked_add(digit)
    })
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn strip_prefix_ascii_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    let candidate = value.get(..prefix.len())?;
    candidate
        .eq_ignore_ascii_case(prefix)
        .then(|| &value[prefix.len()..])
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidPublication(message.into())
}

#[cfg(test)]
#[path = "convert_tests.rs"]
mod tests;
