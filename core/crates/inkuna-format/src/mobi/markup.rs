//! MOBI6 tag-soup normalization into trusted XHTML block content.

use quick_xml::escape::escape;

use super::sanitize::{decoded_xml_text, safe_href, safe_image_src};

pub(crate) struct NormalizedMarkup {
    pub(crate) xhtml: String,
    pub(crate) heading: Option<String>,
}

struct OpenElement {
    name: &'static str,
}

pub(crate) fn normalize(
    input: &str,
    mut image_href: impl FnMut(u32) -> Option<String>,
) -> NormalizedMarkup {
    let mut output = String::with_capacity(input.len());
    let mut stack = Vec::<OpenElement>::new();
    let mut heading_text: Option<String> = None;
    let mut first_heading = None;
    let mut cursor = 0;

    while cursor < input.len() {
        let Some(relative) = input[cursor..].find('<') else {
            append_text(&input[cursor..], &mut output, &mut heading_text);
            break;
        };
        let start = cursor + relative;
        append_text(&input[cursor..start], &mut output, &mut heading_text);

        if input[start..].starts_with("<!--") {
            cursor = input[start + 4..]
                .find("-->")
                .map_or(input.len(), |end| start + 4 + end + 3);
            continue;
        }
        let Some(end) = tag_end(input, start) else {
            append_text("<", &mut output, &mut heading_text);
            cursor = start + 1;
            continue;
        };
        let raw = input[start + 1..end].trim();
        cursor = end + 1;
        if raw.is_empty() || raw.starts_with(['!', '?']) {
            continue;
        }

        let closing = raw.starts_with('/');
        let raw = raw.strip_prefix('/').unwrap_or(raw).trim_start();
        let self_closing = raw.trim_end().ends_with('/');
        let name_end = raw
            .find(|ch: char| ch.is_ascii_whitespace() || ch == '/')
            .unwrap_or(raw.len());
        let source_name = raw[..name_end].to_ascii_lowercase();
        let Some(name) = mapped_name(&source_name) else {
            continue;
        };

        if closing {
            close_through(
                name,
                &mut stack,
                &mut output,
                &mut heading_text,
                &mut first_heading,
            );
            continue;
        }

        autoclose_for(name, &mut stack, &mut output);
        let attrs = parse_attributes(&raw[name_end..]);
        if name == "img" {
            if let Some(tag) = image_tag(&attrs, &mut image_href) {
                output.push_str(&tag);
            }
            continue;
        }

        output.push('<');
        output.push_str(name);
        append_safe_attributes(name, &source_name, &attrs, &mut output);
        if is_void(name) || self_closing {
            output.push_str("/>");
            continue;
        }
        output.push('>');
        if is_heading(name) && first_heading.is_none() && heading_text.is_none() {
            heading_text = Some(String::new());
        }
        stack.push(OpenElement { name });
    }

    while let Some(element) = stack.pop() {
        output.push_str("</");
        output.push_str(element.name);
        output.push('>');
    }
    finish_heading(&mut heading_text, &mut first_heading);
    NormalizedMarkup {
        xhtml: output,
        heading: first_heading,
    }
}

fn tag_end(input: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (relative, ch) in input[start + 1..].char_indices() {
        match (quote, ch) {
            (Some(active), current) if current == active => quote = None,
            (None, '\'' | '"') => quote = Some(ch),
            (None, '>') => return Some(start + 1 + relative),
            _ => {}
        }
    }
    None
}

fn mapped_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "center" => "div",
        "font" => "span",
        "section" | "article" | "aside" | "figure" | "figcaption" => "div",
        "p" => "p",
        "br" => "br",
        "div" => "div",
        "span" => "span",
        "h1" => "h1",
        "h2" => "h2",
        "h3" => "h3",
        "h4" => "h4",
        "h5" => "h5",
        "h6" => "h6",
        "b" => "b",
        "i" => "i",
        "em" => "em",
        "strong" => "strong",
        "u" => "u",
        "s" => "s",
        "small" => "small",
        "sub" => "sub",
        "sup" => "sup",
        "blockquote" => "blockquote",
        "ul" => "ul",
        "ol" => "ol",
        "li" => "li",
        "table" => "table",
        "tr" => "tr",
        "td" => "td",
        "th" => "th",
        "hr" => "hr",
        "img" => "img",
        "a" => "a",
        _ => return None,
    })
}

fn is_void(name: &str) -> bool {
    matches!(name, "br" | "hr" | "img")
}

fn is_heading(name: &str) -> bool {
    matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

fn is_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "blockquote"
            | "ul"
            | "ol"
            | "li"
            | "table"
            | "tr"
            | "td"
            | "th"
    )
}

fn autoclose_for(name: &str, stack: &mut Vec<OpenElement>, output: &mut String) {
    if is_block(name) && stack.iter().any(|element| element.name == "p") {
        close_simple("p", stack, output);
    }
    let peer = match name {
        "p" => Some("p"),
        "li" => Some("li"),
        "tr" => Some("tr"),
        "td" | "th" => stack
            .iter()
            .rev()
            .find(|element| matches!(element.name, "td" | "th"))
            .map(|element| element.name),
        _ => None,
    };
    if let Some(peer) = peer {
        close_simple(peer, stack, output);
    }
}

fn close_simple(name: &str, stack: &mut Vec<OpenElement>, output: &mut String) {
    let Some(index) = stack.iter().rposition(|element| element.name == name) else {
        return;
    };
    for element in stack.drain(index..).rev() {
        output.push_str("</");
        output.push_str(element.name);
        output.push('>');
    }
}

fn close_through(
    name: &str,
    stack: &mut Vec<OpenElement>,
    output: &mut String,
    heading_text: &mut Option<String>,
    first_heading: &mut Option<String>,
) {
    let Some(index) = stack.iter().rposition(|element| element.name == name) else {
        return;
    };
    for element in stack.drain(index..).rev() {
        output.push_str("</");
        output.push_str(element.name);
        output.push('>');
        if is_heading(element.name) {
            finish_heading(heading_text, first_heading);
        }
    }
}

fn finish_heading(buffer: &mut Option<String>, heading: &mut Option<String>) {
    if heading.is_some() {
        *buffer = None;
        return;
    }
    let Some(text) = buffer.take() else {
        return;
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    *heading = Some(trimmed.chars().take(120).collect());
}

fn append_text(input: &str, output: &mut String, heading: &mut Option<String>) {
    if input.is_empty() {
        return;
    }
    let decoded = decoded_xml_text(input);
    output.push_str(&escape(&decoded));
    if let Some(heading) = heading {
        heading.push_str(&decoded);
    }
}

fn parse_attributes(input: &str) -> Vec<(String, String)> {
    let bytes = input.as_bytes();
    let mut attrs = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        while cursor < bytes.len() && (bytes[cursor].is_ascii_whitespace() || bytes[cursor] == b'/')
        {
            cursor += 1;
        }
        let start = cursor;
        while cursor < bytes.len()
            && !bytes[cursor].is_ascii_whitespace()
            && !matches!(bytes[cursor], b'=' | b'/')
        {
            cursor += 1;
        }
        if start == cursor {
            cursor += 1;
            continue;
        }
        let name = input[start..cursor].to_ascii_lowercase();
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let mut value = String::new();
        if cursor < bytes.len() && bytes[cursor] == b'=' {
            cursor += 1;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor < bytes.len() && matches!(bytes[cursor], b'\'' | b'"') {
                let quote = bytes[cursor];
                cursor += 1;
                let value_start = cursor;
                while cursor < bytes.len() && bytes[cursor] != quote {
                    cursor += 1;
                }
                value = decoded_xml_text(&input[value_start..cursor]);
                cursor += usize::from(cursor < bytes.len());
            } else {
                let value_start = cursor;
                while cursor < bytes.len()
                    && !bytes[cursor].is_ascii_whitespace()
                    && bytes[cursor] != b'/'
                {
                    cursor += 1;
                }
                value = decoded_xml_text(&input[value_start..cursor]);
            }
        }
        attrs.push((name, value));
    }
    attrs
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value.as_str())
}

fn append_safe_attributes(
    name: &str,
    source_name: &str,
    attrs: &[(String, String)],
    output: &mut String,
) {
    if source_name == "center" {
        output.push_str(" class=\"mobi-center\"");
    }
    let allowed: &[&str] = match name {
        "a" => &["href", "id"],
        "td" | "th" => &["colspan", "rowspan"],
        _ => &[],
    };
    for name in allowed {
        if let Some(value) = attr(attrs, name) {
            if *name == "href" && !safe_href(value) {
                continue;
            }
            output.push(' ');
            output.push_str(name);
            output.push_str("=\"");
            output.push_str(&escape(value));
            output.push('"');
        }
    }
}

fn image_tag(
    attrs: &[(String, String)],
    image_href: &mut impl FnMut(u32) -> Option<String>,
) -> Option<String> {
    let (src, alt) = if let Some(raw) = attr(attrs, "recindex") {
        let index = raw.trim().parse().ok()?;
        (image_href(index)?, "")
    } else {
        let src = attr(attrs, "src")?;
        if !safe_image_src(src) {
            return None;
        }
        (src.to_string(), attr(attrs, "alt").unwrap_or(""))
    };
    Some(format!(
        "<img src=\"{}\" alt=\"{}\"/>",
        escape(&src),
        escape(alt)
    ))
}

#[cfg(test)]
#[path = "markup_tests.rs"]
mod tests;
