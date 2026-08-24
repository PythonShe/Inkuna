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
    match scheme_of(value) {
        Scheme::Absent => true,
        Scheme::Named(scheme) => {
            scheme.eq_ignore_ascii_case("http")
                || scheme.eq_ignore_ascii_case("https")
                || scheme.eq_ignore_ascii_case("mailto")
        }
        Scheme::Obfuscated => false,
    }
}

pub(crate) fn safe_image_src(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && !value.starts_with('/') && matches!(scheme_of(value), Scheme::Absent)
}

enum Scheme<'a> {
    /// No scheme: a relative reference like `notes/a:b.html`.
    Absent,
    /// A well-formed RFC 3986 scheme.
    Named(&'a str),
    /// A colon-bearing prefix that fails the scheme grammar because it
    /// carries control characters — the shape of `java\nscript:` smuggling,
    /// where a browser strips the controls and sees a scheme after all.
    Obfuscated,
}

/// The RFC 3986 scheme of `value`, if it has one: the text before a colon
/// counts only when that colon precedes any `/`, `?`, or `#` and the text
/// matches `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`. Relative
/// references like `notes/a:b.html` or `page.html?q=a:b` have no scheme.
/// A grammar-failing candidate is scheme-less only when it is free of
/// ASCII control characters; with them it is [`Scheme::Obfuscated`].
fn scheme_of(value: &str) -> Scheme<'_> {
    let Some(colon) = value.find(':') else {
        return Scheme::Absent;
    };
    let candidate = &value[..colon];
    if candidate.contains(['/', '?', '#']) {
        return Scheme::Absent;
    }
    let well_formed = candidate
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic())
        && candidate
            .chars()
            .skip(1)
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'));
    if well_formed {
        Scheme::Named(candidate)
    } else if candidate.chars().any(|ch| ch.is_ascii_control()) {
        Scheme::Obfuscated
    } else {
        Scheme::Absent
    }
}

fn is_xml_char(ch: char) -> bool {
    matches!(ch as u32, 0x09 | 0x0a | 0x0d | 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff)
}

#[cfg(test)]
#[path = "sanitize_tests.rs"]
mod tests;
