//! Bounded byte-level scanning for MOBI6 tags and `filepos` references.

use std::collections::BTreeSet;

use crate::CoreError;

const MAX_SCANNED_TAGS: usize = 1_000_000;
const MAX_FILEPOS_TARGETS: usize = 100_000;
const MAX_TAG_BYTES: usize = 512 * 1024;

pub(super) struct Scan {
    pub(super) tag_starts: Vec<usize>,
    pub(super) block_starts: Vec<usize>,
    pub(super) pagebreaks: Vec<usize>,
    pub(super) links: Vec<FileposLink>,
    pub(super) targets: BTreeSet<usize>,
}

pub(super) struct FileposLink {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) target: usize,
    pub(super) id: Option<String>,
}

pub(super) fn scan_markup(bytes: &[u8]) -> Result<Scan, CoreError> {
    let mut scan = Scan {
        tag_starts: Vec::new(),
        block_starts: Vec::new(),
        pagebreaks: Vec::new(),
        links: Vec::new(),
        targets: BTreeSet::new(),
    };
    let mut cursor = 0;
    while let Some(relative) = bytes[cursor..].iter().position(|byte| *byte == b'<') {
        let start = cursor + relative;
        let Some(end) = raw_tag_end(bytes, start) else {
            break;
        };
        if end - start + 1 > MAX_TAG_BYTES {
            return Err(invalid("MOBI6 tag exceeds the 512 KiB limit"));
        }
        if scan.tag_starts.len() == MAX_SCANNED_TAGS {
            return Err(invalid("MOBI6 markup exceeds the tag scan limit"));
        }
        scan.tag_starts.push(start);
        let raw = trim_ascii(&bytes[start + 1..end]);
        let (name, _) = tag_name(raw);
        if is_block(name) {
            scan.block_starts.push(start);
        }
        if name.eq_ignore_ascii_case(b"mbp:pagebreak") {
            scan.pagebreaks.push(start);
        }
        if let Some(target) = decimal_attr(raw, b"filepos") {
            if !scan.targets.contains(&target) && scan.targets.len() == MAX_FILEPOS_TARGETS {
                return Err(invalid("MOBI6 filepos target count exceeds 100000"));
            }
            scan.targets.insert(target);
            if name.eq_ignore_ascii_case(b"a") {
                scan.links.push(FileposLink {
                    start,
                    end: end + 1,
                    target,
                    id: text_attr(raw, b"id"),
                });
            }
        }
        cursor = end + 1;
    }
    Ok(scan)
}

fn raw_tag_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, byte) in bytes[start + 1..].iter().enumerate() {
        match (quote, byte) {
            (Some(active), current) if current == active => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => return Some(start + 1 + offset),
            _ => {}
        }
    }
    None
}

fn tag_name(raw: &[u8]) -> (&[u8], usize) {
    let offset = usize::from(raw.starts_with(b"/"));
    let body = &raw[offset..];
    let length = body
        .iter()
        .take_while(|byte| !byte.is_ascii_whitespace() && **byte != b'/')
        .count();
    (&body[..length], offset + length)
}

fn is_block(name: &[u8]) -> bool {
    const BLOCKS: &[&[u8]] = &[
        b"p",
        b"div",
        b"h1",
        b"h2",
        b"h3",
        b"h4",
        b"h5",
        b"h6",
        b"blockquote",
        b"ul",
        b"ol",
        b"li",
        b"table",
        b"tr",
    ];
    BLOCKS.iter().any(|block| name.eq_ignore_ascii_case(block))
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    bytes
}

fn decimal_attr(raw: &[u8], name: &[u8]) -> Option<usize> {
    text_attr_bytes(raw, name)
        .and_then(|value| std::str::from_utf8(value).ok())?
        .trim()
        .parse()
        .ok()
}

fn text_attr(raw: &[u8], name: &[u8]) -> Option<String> {
    Some(String::from_utf8_lossy(text_attr_bytes(raw, name)?).into_owned())
}

fn text_attr_bytes<'a>(raw: &'a [u8], wanted: &[u8]) -> Option<&'a [u8]> {
    let mut cursor = tag_name(raw).1;
    while cursor < raw.len() {
        while cursor < raw.len() && (raw[cursor].is_ascii_whitespace() || raw[cursor] == b'/') {
            cursor += 1;
        }
        let start = cursor;
        while cursor < raw.len()
            && !raw[cursor].is_ascii_whitespace()
            && !matches!(raw[cursor], b'=' | b'/')
        {
            cursor += 1;
        }
        let name = &raw[start..cursor];
        while cursor < raw.len() && raw[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= raw.len() || raw[cursor] != b'=' {
            continue;
        }
        cursor += 1;
        while cursor < raw.len() && raw[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let (value_start, value_end) = if cursor < raw.len() && matches!(raw[cursor], b'\'' | b'"')
        {
            let quote = raw[cursor];
            cursor += 1;
            let start = cursor;
            while cursor < raw.len() && raw[cursor] != quote {
                cursor += 1;
            }
            (start, cursor)
        } else {
            let start = cursor;
            while cursor < raw.len() && !raw[cursor].is_ascii_whitespace() && raw[cursor] != b'/' {
                cursor += 1;
            }
            (start, cursor)
        };
        if name.eq_ignore_ascii_case(wanted) {
            return Some(&raw[value_start..value_end]);
        }
        cursor += usize::from(cursor < raw.len());
    }
    None
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidPublication(message.into())
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
