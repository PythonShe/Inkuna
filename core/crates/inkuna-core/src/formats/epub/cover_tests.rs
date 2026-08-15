use super::*;

/// The media type decides whether an item may be persisted as a cover at
/// all. A crafted manifest can flag a content document `cover-image` (or
/// point `<meta name="cover">` at it); before the gate the href fallback
/// derived the perfectly "usable" extension `xhtml` from it, so the whole
/// XHTML document was written into `covers/<id>.xhtml` and served to the
/// shells as artwork.
#[test]
fn a_declared_non_image_is_never_persisted_as_a_cover() {
    assert_eq!(image_extension("application/xhtml+xml", "cover.xhtml"), None);
    assert_eq!(image_extension("text/css", "style.css"), None);
    // The no-suffix branch is refused for the same reason: without it the
    // fallback stored the bytes as `<id>.img`.
    assert_eq!(image_extension("application/xhtml+xml", "cover"), None);
}

/// Refusing non-images must not cost the images the five known types do
/// not name — `image/avif`, `image/heic` and whatever ships next — whose
/// extension still comes from the href.
#[test]
fn declared_images_keep_the_href_fallback() {
    assert_eq!(image_extension("image/jpeg", "表紙.jpg").as_deref(), Some("jpg"));
    assert_eq!(image_extension("image/avif", "表紙.AVIF").as_deref(), Some("avif"));
    assert_eq!(image_extension("image/heic", "表紙").as_deref(), Some("img"));
    // Still rejected: the derived suffix is not a plain short extension,
    // and `covers/<id>.old/cover` would fail on the missing directory.
    assert_eq!(image_extension("image/x-unknown", "img.old/cover"), None);
}

/// Manifest items are required to carry a media type, but real files omit
/// it. With no declaration the href suffix is the only signal left, so it
/// is matched against the known image suffixes rather than trusted.
#[test]
fn a_missing_media_type_falls_back_to_a_known_image_suffix() {
    assert_eq!(image_extension("", "images/表紙.jpeg").as_deref(), Some("jpeg"));
    assert_eq!(image_extension("", "cover.xhtml"), None);
    assert_eq!(image_extension("", "cover"), None);
}
