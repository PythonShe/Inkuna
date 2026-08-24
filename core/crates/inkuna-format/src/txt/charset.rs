//! Charset detection and newline normalization for plain-text imports.

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, UTF_8};

const DETECTION_BYTES: usize = 1024 * 1024;
const UTF16_SAMPLE_BYTES: usize = 4 * 1024;
/// Below four code units a sample carries no usable signal either way.
const UTF16_MIN_SAMPLE_BYTES: usize = 8;
/// Floor on NUL count before the sample is scored at all. One stray NUL in
/// Latin text is not evidence of UTF-16, and short samples of it score well
/// by luck — `text\0then binary` reads as seven CJK ideographs plus a `t`.
const UTF16_MIN_NULS: usize = 4;

pub(super) struct DecodedText {
    pub(super) text: String,
    pub(super) encoding: String,
}

/// Decodes raw bytes to text with normalized `\n` line endings. Charset
/// resolution order: UTF-8/UTF-16 BOM (stripped from the output), the
/// BOM-less UTF-16 heuristic ([`bomless_utf16`]), then a chardetng guess
/// over the first 1 MiB. Decoding is lossy — malformed sequences become
/// U+FFFD, never an error — and the reported encoding is the `encoding_rs`
/// canonical name.
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

/// Recognizes BOM-less UTF-16 over a clamped 4 KiB sample by decoding it
/// both ways and scoring which reading yields more plausible text.
///
/// NUL parity alone cannot answer this. The NULs in ASCII-dominant UTF-16
/// come from the *high* byte of `U+00xx`, but the NULs in CJK-dominant
/// UTF-16 come from the *low* byte of `U+xx00` — 一 (U+4E00), 言 (U+8A00),
/// 退 (U+9000) — which is the opposite parity in the same encoding. Worse,
/// the two classes are byte-swapped images of each other (一 flips to `N`),
/// so a NUL is evidence of UTF-16 but never of its byte order.
///
/// What does discriminate is the rest of the sample: the flipped reading of
/// real text lands in unassigned blocks, control ranges, private use, and
/// unpaired surrogates, while the true reading stays inside the script
/// blocks people actually write in.
///
/// The gate ahead of the scoring is NUL count, not NUL parity: valid UTF-8,
/// GBK, Big5 and Shift_JIS text can never contain a NUL byte, so requiring
/// several of them closes the false-positive path this heuristic would
/// otherwise open. The floor is deliberately low — 一 alone runs about 1.5%
/// of Chinese prose — and the scoring, not the count, is what decides.
/// `None` means "not UTF-16", and is shared by [`decode_text`] and `Format`
/// detection so the two always agree.
pub fn bomless_utf16(sample: &[u8]) -> Option<&'static Encoding> {
    let sample = &sample[..sample.len().min(UTF16_SAMPLE_BYTES)];
    // Score whole code units only; a trailing odd byte is not one.
    let sample = &sample[..sample.len() / 2 * 2];
    if sample.len() < UTF16_MIN_SAMPLE_BYTES {
        return None;
    }
    let unit_count = sample.len() / 2;
    let nuls = sample.iter().filter(|byte| **byte == 0).count();
    if nuls < UTF16_MIN_NULS.max(unit_count / 100) {
        return None;
    }
    let units = unit_count as i32;
    let little = plausibility(sample, true);
    let big = plausibility(sample, false);
    if little == big {
        return None;
    }
    let (encoding, winner, loser) = if little > big {
        (UTF_16LE, little, big)
    } else {
        (UTF_16BE, big, little)
    };
    // The winner has to read as text on its own terms (mean weight at least
    // ½) *and* beat its own byte-swap decisively. Byte-swapped CJK keeps
    // landing on ideographs by luck, so the margin — not the winner's score —
    // is what separates real UTF-16 from a NUL-bearing sample of something
    // else.
    if winner * 2 < units || winner - loser < units / 4 {
        return None;
    }
    Some(encoding)
}

/// Sums [`char_weight`] over the sample read in one byte order. Unpaired
/// surrogates decode to U+FFFD, which the weighting treats as the garbage
/// it is.
fn plausibility(sample: &[u8], little_endian: bool) -> i32 {
    let units = sample.chunks_exact(2).map(|pair| {
        let pair = [pair[0], pair[1]];
        if little_endian {
            u16::from_le_bytes(pair)
        } else {
            u16::from_be_bytes(pair)
        }
    });
    char::decode_utf16(units)
        .map(|unit| char_weight(unit.unwrap_or(char::REPLACEMENT_CHARACTER)))
        .sum()
}

/// +1 for a character real prose is made of, -1 for one it is not, and 0
/// where the evidence is structurally worthless.
fn char_weight(character: char) -> i32 {
    let code_point = character as u32;
    // `U+xx00` is precisely the byte-swapped image of Latin-1, so it is the
    // one class that cannot discriminate: every ASCII character flips into
    // some `U+xx00` ideograph, and 一 flips back into `N`. Neutral, so the
    // remaining characters decide.
    if (0x100..=0xffff).contains(&code_point) && (code_point & 0xff) == 0 {
        return 0;
    }
    match code_point {
        0x09 | 0x0a | 0x0d => 1,
        0x20..=0x7e             // ASCII text
        | 0xa0..=0x24f          // Latin-1 supplement, Latin Extended A/B
        | 0x370..=0x52f         // Greek, Cyrillic
        | 0x590..=0x6ff         // Hebrew, Arabic
        | 0x900..=0x97f         // Devanagari
        | 0xe00..=0xe7f         // Thai
        | 0x2010..=0x205e       // dashes, smart quotes, common punctuation
        | 0x3000..=0x30ff       // CJK punctuation, kana
        | 0x4e00..=0x9fff       // CJK unified ideographs
        | 0xac00..=0xd7a3       // Hangul syllables
        | 0xff01..=0xff60       // fullwidth forms
        | 0x1f300..=0x1faff     // emoji
        | 0x20000..=0x2fa1f     // CJK extension B and beyond
        => 1,
        0x2000..=0x200f         // exotic spaces and joiners
        | 0x3400..=0x4dbf       // CJK extension A: assigned, but rare in books
        | 0xf900..=0xfaff       // CJK compatibility ideographs
        | 0xfe30..=0xfe4f       // CJK compatibility forms
        | 0xff61..=0xff9f       // halfwidth kana
        => 0,
        _ => -1,
    }
}

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace(['\r', '\u{85}', '\u{2028}', '\u{2029}'], "\n")
}

#[cfg(test)]
#[path = "charset_tests.rs"]
mod tests;
