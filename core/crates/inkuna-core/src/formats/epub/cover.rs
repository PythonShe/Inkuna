//! Cover art: mapping a manifest item's media type to the extension the
//! extracted image is stored under.

/// Longest suffix accepted from the href fallback. Every real image
/// extension fits well inside it ("jpeg", "webp").
const MAX_EXTENSION_LEN: usize = 8;

/// The extension the extracted cover is stored under: a known media type
/// maps directly, anything else falls back to the href's suffix.
///
/// Returns `None` when that fallback is not a plain, short, alphanumeric
/// suffix. An href like `img.old/cover` otherwise yields the "extension"
/// `old/cover`, and writing `covers/<id>.old/cover` fails on the missing
/// intermediate directory — failing the whole import over a cover, which
/// is optional data. The caller degrades to no cover instead.
pub(super) fn image_extension(media_type: &str, href: &str) -> Option<String> {
    let known = match media_type {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/svg+xml" => Some("svg"),
        "image/webp" => Some("webp"),
        _ => None,
    };
    if let Some(extension) = known {
        return Some(extension.into());
    }
    // No suffix at all: nothing to validate, and "img" is safe by
    // construction.
    let Some((_, extension)) = href.rsplit_once('.') else {
        return Some("img".into());
    };
    let extension = extension.to_ascii_lowercase();
    let usable = (1..=MAX_EXTENSION_LEN).contains(&extension.len())
        && extension.chars().all(|c| c.is_ascii_alphanumeric());
    usable.then_some(extension)
}
