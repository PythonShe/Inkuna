//! Edition identity: the two normalizers behind the finished-books stat's
//! de-duplication. A book you finished, removed, and re-imported as a
//! differently-hashed copy is one book you finished — content hashing
//! cannot see that, because the two files differ byte for byte.
//!
//! [`edition_key`] is a STRICT ALLOWLIST over `dc:identifier`, and that is
//! the whole design: only a UUID, a checksum-valid ISBN, or a DOI yields an
//! identity. `calibre_id`, `book`, `1`, a bare title, a digit string that
//! fails its check digit, and an empty value all yield `None` — and a book
//! with no identity counts as itself. Under-counting (merging two genuinely
//! distinct books) is a worse error than the double count this closes, so
//! every ambiguous shape is refused rather than guessed at. For the same
//! reason there is deliberately no title+author fallback key: "Selected
//! Poems" across two volumes, or two years of an annual, would silently
//! merge into one.
//!
//! [`title_key`] is the second lock, never an identity on its own: a merge
//! needs both a shared strong identifier AND a byte-identical normalized
//! title, so a broken packing toolchain that stamps one hardcoded
//! `urn:uuid:` on every book it produces still cannot collapse a shelf.

use icu_casemap::CaseMapper;
use icu_normalizer::ComposingNormalizer;

#[cfg(test)]
#[path = "edition_tests.rs"]
mod tests;

/// A publication's edition identity, derived from its OPF
/// `dc:identifier`, or `None` when the identifier is not one of the three
/// recognized schemes. The returned key is scheme-prefixed
/// (`uuid:`/`isbn:`/`doi:`) so two schemes can never collide, and
/// canonical within its scheme: an ISBN-10 and the ISBN-13 of the same
/// edition produce the same key, as do two printings that differ only in
/// hyphenation.
pub fn edition_key(raw: &str) -> Option<String> {
    // The mapper hands back a borrow of compiled data — free to construct.
    let mapper = CaseMapper::new();
    let folded = mapper.fold_string(raw.trim());
    let folded = folded.as_ref();
    // One `urn:` only: `urn:urn:uuid:…` is malformed, not a nested URN.
    let value = folded.strip_prefix("urn:").unwrap_or(folded).trim();

    if let Some(rest) = value.strip_prefix("uuid:") {
        return uuid_key(rest.trim(), false);
    }
    if let Some(rest) = value.strip_prefix("isbn:") {
        return isbn_key(rest.trim());
    }
    if let Some(rest) = value.strip_prefix("doi:") {
        return doi_key(rest.trim());
    }
    // Unprefixed, so the value must *look* like exactly one scheme: an
    // RFC-4122-shaped UUID or a 10/13-character ISBN candidate that
    // survives its check digit. A bare DOI is not accepted — `10.x/y` is
    // not distinctive enough to allowlist without its scheme.
    uuid_key(value, true).or_else(|| isbn_key(value))
}

/// An equality key for a title: NFKC, full Unicode case fold, then every
/// whitespace char removed.
///
/// The CJK reasoning is the point. NFKC maps full-width Latin and digits
/// to half-width, so `ＡＢＣ` and `ABC` agree; it maps half-width katakana
/// `ｶﾀｶﾅ` to `カタカナ`, so a Japanese title typeset either way agrees; and
/// it composes Hangul Jamo, so a decomposed Korean title handed over by a
/// file provider matches a composed one — the same hazard the import
/// pipeline's `nfc` already guards for filename-derived titles.
///
/// It does NOT map Simplified to Traditional Chinese, and that is
/// correct: 简体 and 繁體 are genuinely different editions and must not
/// merge.
///
/// No segmentation, no jieba: this is an equality key, not a search key.
pub fn title_key(title: &str) -> String {
    // Both constructors hand back borrows of compiled data — free to call.
    let normalizer = ComposingNormalizer::new_nfkc();
    let mapper = CaseMapper::new();
    let normalized = normalizer.normalize(title.trim());
    mapper
        .fold_string(&normalized)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// `uuid:<hyphenated lowercase>`, or `None`. The nil and max UUIDs are
/// refused: both are "no identifier" written as one, and tools emit them.
/// With `require_hyphenated`, only the canonical 8-4-4-4-12 form is
/// accepted — the `uuid` crate would otherwise take a bare 32-character
/// hex run, which is not distinctive enough to allowlist unprefixed.
fn uuid_key(value: &str, require_hyphenated: bool) -> Option<String> {
    if require_hyphenated && !is_hyphenated_shape(value) {
        return None;
    }
    let parsed = uuid::Uuid::parse_str(value).ok()?;
    let bytes = *parsed.as_bytes();
    if bytes == [0x00; 16] || bytes == [0xff; 16] {
        return None;
    }
    Some(format!("uuid:{}", parsed.hyphenated()))
}

fn is_hyphenated_shape(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

/// `isbn:<13 digits>`, or `None`. Separators are dropped first (a printed
/// ISBN is hyphenated or spaced), then the check digit must hold; an
/// ISBN-10 is converted to its ISBN-13 so two printings of one edition —
/// one stamped before 2007, one after — merge.
fn isbn_key(value: &str) -> Option<String> {
    let compact: String = value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    match compact.len() {
        10 => isbn10_to_13(&compact),
        13 => isbn13_key(&compact),
        _ => None,
    }
}

fn isbn13_key(compact: &str) -> Option<String> {
    let digits: Vec<u32> = compact
        .chars()
        .map(|c| c.to_digit(10))
        .collect::<Option<_>>()?;
    let sum: u32 = digits
        .iter()
        .enumerate()
        .map(|(i, d)| if i % 2 == 0 { *d } else { d * 3 })
        .sum();
    (sum % 10 == 0).then(|| format!("isbn:{compact}"))
}

fn isbn10_to_13(compact: &str) -> Option<String> {
    let mut digits: Vec<u32> = Vec::with_capacity(10);
    for (i, c) in compact.chars().enumerate() {
        // Only the check position may be `x` (already case-folded to
        // lowercase), and it means 10.
        match (i, c) {
            (9, 'x') => digits.push(10),
            _ => digits.push(c.to_digit(10)?),
        }
    }
    let sum: u32 = digits
        .iter()
        .enumerate()
        .map(|(i, d)| (10 - i as u32) * d)
        .sum();
    if sum % 11 != 0 {
        return None;
    }
    let mut thirteen = String::with_capacity(13);
    thirteen.push_str("978");
    for d in &digits[..9] {
        thirteen.push(char::from_digit(*d, 10)?);
    }
    let sum: u32 = thirteen
        .chars()
        .filter_map(|c| c.to_digit(10))
        .enumerate()
        .map(|(i, d)| if i % 2 == 0 { d } else { d * 3 })
        .sum();
    thirteen.push(char::from_digit((10 - sum % 10) % 10, 10)?);
    Some(format!("isbn:{thirteen}"))
}

/// `doi:10.<registrant>/<suffix>`, or `None`. The registrant is digits
/// (dots allowed, for the sub-registrant form `10.1000.10/…`) and the
/// suffix must be non-empty; the value is already case-folded, which is
/// what makes two spellings of one DOI equal.
fn doi_key(value: &str) -> Option<String> {
    let rest = value.strip_prefix("10.")?;
    let (registrant, suffix) = rest.split_once('/')?;
    if suffix.is_empty() || suffix.chars().any(char::is_whitespace) {
        return None;
    }
    if !registrant.chars().any(|c| c.is_ascii_digit())
        || !registrant.chars().all(|c| c.is_ascii_digit() || c == '.')
    {
        return None;
    }
    Some(format!("doi:{value}"))
}
