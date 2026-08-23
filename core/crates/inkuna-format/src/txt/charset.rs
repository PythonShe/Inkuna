//! Charset detection and newline normalization for plain-text imports.

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, UTF_8};

const DETECTION_BYTES: usize = 1024 * 1024;
const UTF16_SAMPLE_BYTES: usize = 4 * 1024;

pub(super) struct DecodedText {
    pub(super) text: String,
    pub(super) encoding: String,
}

/// Decodes raw bytes to text with normalized `\n` line endings. Charset
/// resolution order: UTF-8/UTF-16 BOM (stripped from the output), the
/// BOM-less UTF-16 NUL-density heuristic ([`bomless_utf16`]), then a
/// chardetng guess over the first 1 MiB. Decoding is lossy — malformed
/// sequences become U+FFFD, never an error — and the reported encoding
/// is the `encoding_rs` canonical name.
pub(super) fn decode_text(bytes: &[u8]) -> DecodedText {
    let (encoding, content) = if let Some(content) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        (UTF_8, content)
    } else if let Some(content) = bytes.strip_prefix(&[0xff, 0xfe]) {
        (UTF_16LE, content)
    } else if let Some(content) = bytes.strip_prefix(&[0xfe, 0xff]) {
        (UTF_16BE, content)
    } else if let Some(encoding) = bomless_utf16(bytes) {
        (encoding, bytes)
    } else {
        let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
        let sample = &bytes[..bytes.len().min(DETECTION_BYTES)];
        detector.feed(sample, true);
        (detector.guess(None, Utf8Detection::Allow), bytes)
    };

    let (text, _, _) = encoding.decode(content);
    DecodedText {
        text: normalize_line_endings(&text),
        encoding: encoding.name().to_string(),
    }
}

/// Recognizes BOM-less UTF-16 from NUL density: over a clamped 4 KiB
/// sample, the dominant parity must contain at least 1% NULs and at least
/// four times as many as the other parity. Valid UTF-8 is never treated as
/// UTF-16 unless its alternating NULs are the unambiguous byte layout of
/// ASCII UTF-16. `None` means "not UTF-16", including for a NUL-free sample —
/// shared by [`decode_text`] and `Format` detection so the two always
/// agree.
pub fn bomless_utf16(sample: &[u8]) -> Option<&'static Encoding> {
    let sample = &sample[..sample.len().min(UTF16_SAMPLE_BYTES)];
    if sample.is_empty() {
        return None;
    }
    let (even, odd) = sample
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == 0)
        .fold((0usize, 0usize), |(even, odd), (index, _)| {
            if index % 2 == 0 {
                (even + 1, odd)
            } else {
                (even, odd + 1)
            }
        });
    let dominant = even.max(odd);
    let other = even.min(odd);
    if dominant * 100 < sample.len() || dominant < other * 4 {
        return None;
    }
    // An ASCII UTF-16 stream is technically valid UTF-8 because NUL is a
    // legal UTF-8 code point. Preserve that unambiguous layout while
    // rejecting every other valid UTF-8 sample before it can be a false
    // positive under the lower NUL floor.
    if std::str::from_utf8(sample).is_ok() && !is_ascii_utf16(sample, even > odd) {
        return None;
    }
    if even == odd {
        None
    } else {
        Some(if even > odd { UTF_16BE } else { UTF_16LE })
    }
}

fn is_ascii_utf16(sample: &[u8], big_endian: bool) -> bool {
    let (high, low) = if big_endian { (0, 1) } else { (1, 0) };
    sample.len().is_multiple_of(2)
        && sample
            .chunks_exact(2)
            .all(|pair| pair[high] == 0 && pair[low].is_ascii())
}

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace(['\r', '\u{85}', '\u{2028}', '\u{2029}'], "\n")
}

#[cfg(test)]
#[path = "charset_tests.rs"]
mod tests;
