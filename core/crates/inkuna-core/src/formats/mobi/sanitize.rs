//! XML-character and URI safety shared by MOBI6 metadata and markup.

use super::entities::decode_entities;

pub(crate) fn decoded_xml_text(input: &str) -> String {
    filter_xml_chars(&decode_entities(input))
}

pub(crate) fn filter_xml_chars(input: &str) -> String {
    input.chars().filter(|ch| is_xml_char(*ch)).collect()
}

pub(crate) fn safe_href(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || value.starts_with("//") {
        return false;
    }
    let Some(scheme) = scheme_of(value) else {
        return true;
    };
    scheme.eq_ignore_ascii_case("http")
        || scheme.eq_ignore_ascii_case("https")
        || scheme.eq_ignore_ascii_case("mailto")
}

pub(crate) fn safe_image_src(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && !value.starts_with('/') && scheme_of(value).is_none()
}

/// The RFC 3986 scheme of `value`, if it has one: the text before a colon
/// counts only when that colon precedes any `/`, `?`, or `#` and the text
/// matches `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`. Relative
/// references like `notes/a:b.html` or `page.html?q=a:b` have no scheme.
fn scheme_of(value: &str) -> Option<&str> {
    let colon = value.find(':')?;
    let candidate = &value[..colon];
    if candidate.contains(['/', '?', '#']) {
        return None;
    }
    let mut chars = candidate.chars();
    if !chars.next()?.is_ascii_alphabetic() {
        return None;
    }
    chars
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
        .then_some(candidate)
}

fn is_xml_char(ch: char) -> bool {
    matches!(ch as u32, 0x09 | 0x0a | 0x0d | 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff)
}

#[cfg(test)]
#[path = "sanitize_tests.rs"]
mod tests;
