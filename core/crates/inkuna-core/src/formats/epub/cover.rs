//! Cover art: mapping a manifest item's media type to the extension the
//! extracted image is stored under.

/// Longest suffix accepted from the href fallback. Every real image
/// extension fits well inside it ("jpeg", "webp").
const MAX_EXTENSION_LEN: usize = 8;

/// Href suffixes accepted as evidence of an image when the manifest item
/// declares no media type at all. Deliberately a whitelist: the suffix is
/// the only signal left, so anything unrecognized means no cover rather
/// than a guess.
const IMAGE_SUFFIXES: &[&str] = &[
    "jpg", "jpeg", "jpe", "png", "gif", "svg", "webp", "avif", "heic", "heif", "bmp", "tif",
    "tiff", "jxl",
];

/// The extension a known image media type maps to directly.
fn known_extension(media_type: &str) -> Option<&'static str> {
    match media_type {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/svg+xml" => Some("svg"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

/// Whether an href suffix is safe to use verbatim as a file extension:
/// short and plain ASCII alphanumeric. An href like `img.old/cover`
/// otherwise yields the "extension" `old/cover`, and writing
/// `covers/<id>.old/cover` fails on the missing intermediate directory —
/// failing the whole import over a cover, which is optional data.
fn usable_suffix(suffix: &str) -> bool {
    (1..=MAX_EXTENSION_LEN).contains(&suffix.len())
        && suffix.chars().all(|c| c.is_ascii_alphanumeric())
}

/// The extension the extracted cover is stored under, or `None` when the
/// item must not be persisted as a cover at all — the caller degrades to
/// no cover, never failing the import.
///
/// The media type decides first, because it is the only statement of what
/// the bytes *are*: a known image type maps directly, any other `image/*`
/// type falls back to the href's suffix (so `image/avif` and friends still
/// work). A declared non-image type — `application/xhtml+xml` on an item a
/// crafted manifest flagged `cover-image`, or pointed at by
/// `<meta name="cover">` — is refused outright; without that gate the href
/// fallback happily persisted a content document as `<id>.xhtml`. A
/// missing media type (non-conforming, but real files do it) leaves the
/// href suffix as the only signal, so it is matched against
/// [`IMAGE_SUFFIXES`] instead of being taken on trust.
pub(super) fn image_extension(media_type: &str, href: &str) -> Option<String> {
    if let Some(extension) = known_extension(media_type) {
        return Some(extension.into());
    }
    let suffix = href
        .rsplit_once('.')
        .map(|(_, suffix)| suffix.to_ascii_lowercase());
    if media_type.starts_with("image/") {
        // Declared an image, just not one of the five above. No suffix at
        // all: nothing to validate, and "img" is safe by construction.
        let Some(suffix) = suffix else {
            return Some("img".into());
        };
        return usable_suffix(&suffix).then_some(suffix);
    }
    if !media_type.is_empty() {
        return None;
    }
    let suffix = suffix?;
    IMAGE_SUFFIXES.contains(&suffix.as_str()).then_some(suffix)
}

#[cfg(test)]
#[path = "cover_tests.rs"]
mod tests;
