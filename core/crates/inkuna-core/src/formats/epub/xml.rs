//! Small XML helpers shared by every parser in this module: entity
//! resolution, whitespace-collapsing text accumulation, attribute lookup.

use quick_xml::events::BytesStart;

/// Resolves an entity-reference event: character references, the XML
/// predefined five, and (EPUB XHTML being HTML-flavored in practice) the
/// HTML5 named set. Unresolvable references are kept verbatim so no
/// content silently disappears.
pub(super) fn resolve_ref(r: &quick_xml::events::BytesRef) -> String {
    if let Ok(Some(ch)) = r.resolve_char_ref() {
        return ch.to_string();
    }
    if let Ok(name) = r.decode() {
        if let Some(resolved) = quick_xml::escape::resolve_predefined_entity(&name)
            .or_else(|| quick_xml::escape::resolve_html5_entity(&name))
        {
            return resolved.to_string();
        }
        return format!("&{name};");
    }
    String::new()
}

/// Appends `text` to `acc`, collapsing runs of whitespace into single
/// spaces. UTF-8-safe: operates on `char` boundaries only, never byte
/// offsets. (Entities never appear here — the parser emits them as
/// separate `GeneralRef` events.)
pub(super) fn push_word(acc: &mut String, text: &str) {
    let mut needs_sep = text.starts_with(char::is_whitespace);
    for word in text.split_whitespace() {
        if needs_sep && !acc.is_empty() && !acc.ends_with(' ') {
            acc.push(' ');
        }
        acc.push_str(word);
        needs_sep = true;
    }
    // Preserve a trailing separator when the raw text ended in whitespace,
    // so `push_word("a "), push_word("b")` stays two words — while text
    // split only by markup or entity boundaries joins seamlessly.
    if text.ends_with(char::is_whitespace) && !acc.is_empty() && !acc.ends_with(' ') {
        acc.push(' ');
    }
}

/// Trims a captured text value; `None` when empty.
pub(super) fn clean_text(text: Option<&str>) -> Option<String> {
    let trimmed = text?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// An attribute's entity-unescaped value by exact local-name match.
pub(super) fn attr_value(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|attr| {
        if attr.key.local_name().as_ref() == name {
            attr.normalized_value(quick_xml::XmlVersion::default())
                .ok()
                .map(|v| v.into_owned())
        } else {
            None
        }
    })
}

/// An attribute's value by raw qualified name (e.g. `epub:type`, where the
/// prefix is significant and `local_name` would collide with other vocab).
pub(super) fn raw_attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|attr| {
        if attr.key.as_ref() == name {
            attr.normalized_value(quick_xml::XmlVersion::default())
                .ok()
                .map(|v| v.into_owned())
        } else {
            None
        }
    })
}
