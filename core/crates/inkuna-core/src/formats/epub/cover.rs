//! Cover art: mapping a manifest item's media type to the extension the
//! extracted image is stored under.

pub(super) fn image_extension(media_type: &str, href: &str) -> String {
    match media_type {
        "image/jpeg" => "jpg".into(),
        "image/png" => "png".into(),
        "image/gif" => "gif".into(),
        "image/svg+xml" => "svg".into(),
        "image/webp" => "webp".into(),
        _ => href
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_else(|| "img".into()),
    }
}
