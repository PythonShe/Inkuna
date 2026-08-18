//! Charset detection and newline normalization for plain-text imports.

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, UTF_8};

const DETECTION_BYTES: usize = 1024 * 1024;
const UTF16_SAMPLE_BYTES: usize = 4 * 1024;

pub(super) struct DecodedText {
    pub(super) text: String,
    pub(super) encoding: String,
}

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

fn bomless_utf16(bytes: &[u8]) -> Option<&'static Encoding> {
    let sample = &bytes[..bytes.len().min(UTF16_SAMPLE_BYTES)];
    if sample.is_empty() {
        return None;
    }
    let nulls = sample.iter().filter(|byte| **byte == 0).count();
    if nulls * 10 <= sample.len() * 3 {
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
    if even == odd {
        None
    } else {
        Some(if even > odd { UTF_16BE } else { UTF_16LE })
    }
}

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace(['\r', '\u{85}', '\u{2028}', '\u{2029}'], "\n")
}

#[cfg(test)]
#[path = "charset_tests.rs"]
mod tests;
