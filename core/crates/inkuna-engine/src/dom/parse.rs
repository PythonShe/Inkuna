//! Lenient streaming XHTML → arena parse. Recovery, never rejection, for
//! anything milder than "no root element at all": unclosed inline elements
//! close at their parent's end, stray ampersands and unknown entities stay
//! literal text, mismatched end tags pop to the nearest matching open
//! element or are dropped. Budgets truncate, they never error.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::arena::{Document, ElementData, ElementName, Node, NodeId, NodeKind, StylesheetSource};
use crate::EngineError;

/// Total arena nodes (elements + text) retained per resource.
pub const MAX_DOM_NODES: usize = 262_144;
/// Sum of text-node bytes retained per resource.
pub const MAX_TEXT_BYTES: usize = 4_194_304;
/// Cap per attribute value; longer values truncate on a char boundary.
pub const MAX_ATTR_BYTES: usize = 4_096;
/// Sum of inline `<style>` bytes retained per resource (linked sheets are
/// budgeted where they are loaded).
pub const MAX_STYLESHEET_BYTES: usize = 1_048_576;
/// Elements deeper than this flatten into their ancestor at this depth.
pub const MAX_DEPTH: usize = 256;

/// Upper bound on recoverable parse errors before the tree so far is kept
/// as-is; guards against a pathological byte stream that errors forever.
const MAX_PARSE_ERRORS: u32 = 128;

/// One open element: its lowercased local name for end-tag matching, and
/// its arena node — `None` when the element was flattened at [`MAX_DEPTH`].
struct Open {
    local: Vec<u8>,
    node: Option<NodeId>,
}

/// Parses one XHTML resource into an arena [`Document`].
///
/// Fails closed — [`EngineError::UnsupportedContent`] — only when no root
/// element is decodable at all; anything milder recovers or truncates.
pub fn parse(xhtml: &[u8]) -> Result<Document, EngineError> {
    let mut reader = Reader::from_reader(xhtml);
    let config = reader.config_mut();
    config.check_end_names = false;
    config.allow_dangling_amp = true;
    let mut buf = Vec::new();

    let mut b = Builder::default();
    let mut errors = 0u32;
    let mut last_error_pos = u64::MAX;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if b.start(&e, false) {
                    break;
                }
            }
            Ok(Event::Empty(e)) => {
                if b.start(&e, true) {
                    break;
                }
            }
            Ok(Event::Text(t)) => {
                if let Ok(text) = t.decode() {
                    if b.text(&text) {
                        break;
                    }
                }
            }
            Ok(Event::CData(t)) => {
                if let Ok(text) = std::str::from_utf8(&t) {
                    if b.text(text) {
                        break;
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if b.text(&resolve_ref(&r)) {
                    break;
                }
            }
            Ok(Event::End(e)) => b.end(e.local_name().as_ref()),
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) if b.root.is_none() => {
                return Err(EngineError::UnsupportedContent {
                    detail: "resource is not decodable XML before any element".to_string(),
                });
            }
            Err(_) => {
                // Mid-document error: keep going while the reader still
                // advances, so a localized flaw costs its neighborhood,
                // not the chapter.
                errors += 1;
                let pos = reader.buffer_position();
                if errors > MAX_PARSE_ERRORS || pos == last_error_pos {
                    break;
                }
                last_error_pos = pos;
            }
        }
        buf.clear();
    }

    let Some(root) = b.root else {
        return Err(EngineError::UnsupportedContent {
            detail: "no root element in resource".to_string(),
        });
    };
    Ok(Document {
        nodes: b.nodes,
        root,
        anchors: b.anchors,
        stylesheets: b.stylesheets,
        truncated: b.truncated,
    })
}

#[derive(Default)]
struct Builder {
    nodes: Vec<Node>,
    root: Option<NodeId>,
    anchors: Vec<(String, NodeId)>,
    stylesheets: Vec<StylesheetSource>,
    truncated: bool,
    stack: Vec<Open>,
    text_bytes: usize,
    style_bytes: usize,
    /// Open `script`/`template` elements (their subtrees are dropped).
    skip_depth: u32,
    /// Open elements inside `<head>` (no arena nodes; `<style>`/`<link>`
    /// are scanned for stylesheet sources). `0` = not in head.
    head_depth: u32,
    /// Body of the `<style>` element currently being captured.
    style_text: Option<String>,
}

impl Builder {
    /// Handles a start (or self-closing) tag. Returns `true` when a
    /// budget stopped the parse.
    fn start(&mut self, e: &BytesStart, empty: bool) -> bool {
        let local = e.local_name().as_ref().to_ascii_lowercase();

        if self.style_text.is_some() {
            return false; // markup inside <style> is not CSS; drop it
        }
        if self.skip_depth > 0 {
            if !empty && matches!(local.as_slice(), b"script" | b"template") {
                self.skip_depth += 1;
            }
            return false;
        }
        if matches!(local.as_slice(), b"script" | b"template") {
            if !empty {
                self.skip_depth = 1;
            }
            return false;
        }
        if local == b"style" {
            if empty {
                return false;
            }
            self.style_text = Some(String::new());
            if self.head_depth > 0 {
                self.head_depth += 1;
            }
            return false;
        }
        if self.head_depth > 0 {
            if local == b"link" {
                self.link(e);
            }
            if !empty {
                self.head_depth += 1;
            }
            return false;
        }
        if local == b"head" {
            if !empty {
                self.head_depth = 1;
            }
            return false;
        }

        // A normal element. Budget first, placement second.
        if self.nodes.len() >= MAX_DOM_NODES {
            self.truncated = true;
            return true;
        }
        let parent = self.current_parent();
        if self.stack.len() >= MAX_DEPTH {
            // Flattened: children attach to the ancestor at MAX_DEPTH;
            // an id here still anchors, at that ancestor.
            if let (Some(id), Some(parent)) = (attr(e, b"id"), parent) {
                self.anchors.push((id, parent));
            }
            if !empty {
                self.stack.push(Open { local, node: None });
            }
            return false;
        }

        let name = element_name(&local);
        let data = self.element_data(e, name);
        let node = NodeId(self.nodes.len() as u32);
        if let Some(id) = &data.id {
            self.anchors.push((id.to_string(), node));
        }
        self.nodes.push(Node {
            kind: NodeKind::Element(data),
            parent,
            children: Vec::new(),
        });
        if let Some(parent) = parent {
            self.nodes[parent.0 as usize].children.push(node);
        }
        if self.root.is_none() {
            self.root = Some(node);
        }
        if !empty {
            self.stack.push(Open {
                local,
                node: Some(node),
            });
        }
        false
    }

    /// Handles an end tag.
    fn end(&mut self, local: &[u8]) {
        let local = local.to_ascii_lowercase();
        if self.style_text.is_some() {
            if local == b"style" {
                let text = self.style_text.take().unwrap_or_default();
                self.stylesheets.push(StylesheetSource::Inline(text));
                self.head_depth = self.head_depth.saturating_sub(1);
            }
            return;
        }
        if self.skip_depth > 0 {
            if matches!(local.as_slice(), b"script" | b"template") {
                self.skip_depth -= 1;
            }
            return;
        }
        if self.head_depth > 0 {
            self.head_depth -= 1;
            return;
        }
        // Pop to the nearest matching open element; drop if none matches.
        if let Some(at) = self.stack.iter().rposition(|open| open.local == local) {
            self.stack.truncate(at);
        }
    }

    /// Handles character data. Returns `true` when the text budget
    /// stopped the parse.
    fn text(&mut self, text: &str) -> bool {
        if let Some(style) = &mut self.style_text {
            let room = MAX_STYLESHEET_BYTES.saturating_sub(self.style_bytes);
            let kept = truncate_boundary(text, room);
            style.push_str(kept);
            self.style_bytes += kept.len();
            return false;
        }
        if self.skip_depth > 0 || self.head_depth > 0 {
            return false;
        }
        let Some(parent) = self.current_parent() else {
            return false; // prolog whitespace before the root element
        };
        let room = MAX_TEXT_BYTES.saturating_sub(self.text_bytes);
        let kept = truncate_boundary(text, room);
        let over_budget = kept.len() < text.len();
        if !kept.is_empty() {
            self.text_bytes += kept.len();
            // Merge with a preceding text sibling so entity references
            // and markup seams never split one run of characters.
            let last = self.nodes[parent.0 as usize].children.last().copied();
            match last.map(|id| &mut self.nodes[id.0 as usize].kind) {
                Some(NodeKind::Text(existing)) => existing.push_str(kept),
                _ => {
                    if self.nodes.len() >= MAX_DOM_NODES {
                        self.truncated = true;
                        return true;
                    }
                    let node = NodeId(self.nodes.len() as u32);
                    self.nodes.push(Node {
                        kind: NodeKind::Text(kept.to_string()),
                        parent: Some(parent),
                        children: Vec::new(),
                    });
                    self.nodes[parent.0 as usize].children.push(node);
                }
            }
        }
        if over_budget {
            self.truncated = true;
            return true;
        }
        false
    }

    /// The node new content attaches to: the nearest open element that
    /// has one, else the root (stray content after `</html>`).
    fn current_parent(&self) -> Option<NodeId> {
        self.stack
            .iter()
            .rev()
            .find_map(|open| open.node)
            .or(self.root)
    }

    /// Captures a `<link rel="stylesheet" href>` (any case; `type` is
    /// ignored) as a linked stylesheet source.
    fn link(&mut self, e: &BytesStart) {
        let is_stylesheet = attr(e, b"rel").is_some_and(|rel| {
            rel.split_ascii_whitespace()
                .any(|word| word.eq_ignore_ascii_case("stylesheet"))
        });
        if !is_stylesheet {
            return;
        }
        if let Some(href) = attr(e, b"href") {
            self.stylesheets.push(StylesheetSource::Linked(href));
        }
    }

    /// Extracts the honored attributes of one element.
    fn element_data(&self, e: &BytesStart, name: ElementName) -> ElementData {
        let href = match name {
            ElementName::A => attr(e, b"href"),
            ElementName::Img => attr(e, b"src"),
            ElementName::Image => attr(e, b"href").or_else(|| attr(e, b"src")),
            _ => None,
        };
        let dir_rtl = attr(e, b"dir").and_then(|dir| {
            if dir.eq_ignore_ascii_case("rtl") {
                Some(true)
            } else if dir.eq_ignore_ascii_case("ltr") {
                Some(false)
            } else {
                None
            }
        });
        ElementData {
            name,
            class: attr(e, b"class").map(Into::into),
            id: attr(e, b"id").map(Into::into),
            lang: attr(e, b"lang").map(Into::into),
            style_attr: attr(e, b"style").map(Into::into),
            href: href.map(Into::into),
            dir_rtl,
        }
    }
}

/// An attribute value by lowercase local name (so `xml:lang` matches
/// `lang`), entity-normalized, budgeted to [`MAX_ATTR_BYTES`] on a char
/// boundary.
fn attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if !a.key.local_name().as_ref().eq_ignore_ascii_case(name) {
            return None;
        }
        let value = a.normalized_value(quick_xml::XmlVersion::default()).ok()?;
        Some(truncate_boundary(&value, MAX_ATTR_BYTES).to_string())
    })
}

/// The longest prefix of `text` at most `max` bytes long that ends on a
/// char boundary.
fn truncate_boundary(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Interns an element's lowercased local name; `i` → `Em`, `b` → `Strong`.
fn element_name(local: &[u8]) -> ElementName {
    match local {
        b"html" => ElementName::Html,
        b"body" => ElementName::Body,
        b"h1" => ElementName::H1,
        b"h2" => ElementName::H2,
        b"h3" => ElementName::H3,
        b"h4" => ElementName::H4,
        b"h5" => ElementName::H5,
        b"h6" => ElementName::H6,
        b"p" => ElementName::P,
        b"div" => ElementName::Div,
        b"span" => ElementName::Span,
        b"em" | b"i" => ElementName::Em,
        b"strong" | b"b" => ElementName::Strong,
        b"blockquote" => ElementName::Blockquote,
        b"pre" => ElementName::Pre,
        b"code" => ElementName::Code,
        b"ol" => ElementName::Ol,
        b"ul" => ElementName::Ul,
        b"li" => ElementName::Li,
        b"ruby" => ElementName::Ruby,
        b"rt" => ElementName::Rt,
        b"rb" => ElementName::Rb,
        b"img" => ElementName::Img,
        b"image" => ElementName::Image,
        b"table" => ElementName::Table,
        b"tr" => ElementName::Tr,
        b"td" => ElementName::Td,
        b"th" => ElementName::Th,
        b"caption" => ElementName::Caption,
        b"br" => ElementName::Br,
        b"hr" => ElementName::Hr,
        b"a" => ElementName::A,
        b"section" => ElementName::Section,
        b"article" => ElementName::Article,
        b"figure" => ElementName::Figure,
        b"figcaption" => ElementName::Figcaption,
        b"dt" => ElementName::Dt,
        b"dd" => ElementName::Dd,
        other => ElementName::Other(String::from_utf8_lossy(other).into()),
    }
}

/// Resolves an entity-reference event: character references, the XML
/// predefined five, and (EPUB XHTML being HTML-flavored in practice) the
/// HTML5 named set. Unresolvable references are kept verbatim so no
/// content silently disappears — mirrors inkuna-content's `xml.rs`.
fn resolve_ref(r: &quick_xml::events::BytesRef) -> String {
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
