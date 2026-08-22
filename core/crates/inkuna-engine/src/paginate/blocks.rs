//! Block extraction: the styled DOM + canonical projection → a flat
//! block sequence in document order, with each paragraph block's
//! resolved style facts. Shaping the blocks lives in
//! [`super::shaping`].
//!
//! Paragraph boundaries are derived from the projection: consecutive
//! spans sharing a nearest block ancestor form one paragraph, exactly
//! mirroring where the projection emitted its `\n` separators (so a
//! `<br>` newline stays inside its paragraph as a forced break, while
//! block edges split). Table content degrades sequentially — each
//! tr/caption is its own paragraph block. List items carry no markers,
//! per the projection.

use std::ops::Range;

use crate::dom::{Document, ElementName, NodeId, NodeKind};
use crate::style::{Direction, StyledDocument, TextAlign};
use crate::text::Projection;

use super::pages::ChapterInput;

/// One block of the chapter's flow, in document order.
pub(super) enum BlockKind {
    Paragraph(ParagraphMeta),
    /// An `hr`: a Rule decoration block.
    Rule,
    /// An `img`/`image` with its verbatim `src`; placement lands with
    /// the images task, collection here preserves document order.
    Image { src: String },
}

/// A paragraph block's identity and resolved style facts.
pub(super) struct ParagraphMeta {
    /// The nearest block ancestor all the paragraph's spans share.
    pub node: NodeId,
    /// Canonical char range, extended over the trailing block
    /// separator so paragraph ranges partition `0..char_len`.
    pub char_range: Range<u64>,
    /// The paragraph's spans, as indices into `projection.spans`.
    pub span_range: Range<usize>,
    pub align: TextAlign,
    pub base_rtl: bool,
    /// h1..h6 as 0..=5, from the nearest heading ancestor.
    pub heading: Option<usize>,
    /// Nested blockquote count: each level insets 1.5 em per side.
    pub quote_depth: u32,
    /// Nearest ancestor `lang`/`xml:lang`.
    pub lang: Option<String>,
    /// First-line paragraph indent applies: body paragraphs only
    /// (p/div-derived text, including bare section/article/body text) —
    /// never headings, pre/code, or blockquote/li/dt/dd/tr/caption
    /// blocks.
    pub indent: bool,
}

/// Walks the chapter into its block sequence.
pub(super) fn collect_blocks(input: &ChapterInput<'_>) -> Vec<BlockKind> {
    let styled = input.styled;
    let doc = styled.doc;
    let proj = input.projection;

    // Paragraphs: maximal runs of spans sharing a block ancestor.
    let mut paras: Vec<(NodeId, Range<usize>)> = Vec::new();
    for (i, span) in proj.spans.iter().enumerate() {
        let key = nearest_block(doc, span.node);
        match paras.last_mut() {
            Some((k, r)) if *k == key => r.end = i + 1,
            _ => paras.push((key, i..i + 1)),
        }
    }

    // hr/img in document order, pinned to the projection offset of the
    // next projected text (their boundary between surrounding chars).
    let mut specials: Vec<(u64, BlockKind)> = Vec::new();
    let mut ptr = 0usize;
    walk_specials(styled, doc.root, &mut ptr, proj, &mut specials);

    // Merge: a special before a paragraph's first char precedes it; a
    // special inside a paragraph's range follows it (v1: images and
    // rules are block-level, never split a paragraph's lines).
    let mut blocks = Vec::new();
    let mut sp = specials.into_iter().peekable();
    for (pi, (key, span_range)) in paras.iter().enumerate() {
        let span_start = proj.spans[span_range.start].char_range.start;
        while let Some((at, _)) = sp.peek() {
            if *at <= span_start {
                if let Some((_, kind)) = sp.next() {
                    blocks.push(kind);
                }
            } else {
                break;
            }
        }
        let start = if pi == 0 { 0 } else { span_start };
        let end = paras
            .get(pi + 1)
            .map_or(proj.char_len, |(_, r)| proj.spans[r.start].char_range.start);
        blocks.push(BlockKind::Paragraph(meta(
            styled,
            proj,
            *key,
            start..end,
            span_range.clone(),
        )));
    }
    for (_, kind) in sp {
        blocks.push(kind);
    }
    blocks
}

/// Resolves one paragraph's style facts from its block node's computed
/// style and ancestor chain.
fn meta(
    styled: &StyledDocument<'_>,
    proj: &Projection,
    key: NodeId,
    char_range: Range<u64>,
    span_range: Range<usize>,
) -> ParagraphMeta {
    let doc = styled.doc;
    let style = styled.styles[key.0 as usize];
    let mut heading = None;
    let mut quote_depth = 0u32;
    let mut pre = false;
    let mut lang = None;
    let mut cur = Some(key);
    while let Some(id) = cur {
        if let Some(el) = doc.element(id) {
            if heading.is_none() {
                heading = heading_level(&el.name);
            }
            match el.name {
                ElementName::Blockquote => quote_depth += 1,
                ElementName::Pre | ElementName::Code => pre = true,
                _ => {}
            }
            if lang.is_none() {
                lang = el.lang.as_deref().map(str::to_string);
            }
        }
        cur = doc.node(id).parent;
    }
    // A `code` element functioning as a block: `code` is not in the
    // block set, so it never keys a paragraph — but when every span of
    // the paragraph sits inside one, the paragraph is code content.
    let pre = pre || spans_all_in_code(doc, proj, &span_range);
    // First-line indent: body paragraphs only. Headings and pre/code
    // are excluded above; the remaining non-body block kinds are
    // excluded by name (blockquote text, list items, definition terms/
    // descriptions, degraded table rows, captions).
    let indent = heading.is_none()
        && !pre
        && !matches!(
            doc.element(key).map(|el| &el.name),
            Some(
                ElementName::Blockquote
                    | ElementName::Li
                    | ElementName::Dt
                    | ElementName::Dd
                    | ElementName::Tr
                    | ElementName::Caption
                    | ElementName::Figcaption
            )
        );
    ParagraphMeta {
        node: key,
        char_range,
        span_range,
        // pre/code: never justified, `Start` align. Interior whitespace
        // renders collapsed in v1 because the canonical projection
        // already collapsed it.
        align: if pre { TextAlign::Start } else { style.text_align },
        base_rtl: style.direction == Direction::Rtl,
        heading,
        quote_depth,
        lang,
        indent,
    }
}

/// True when every span of the paragraph has a `code` ancestor below
/// its block — the whole paragraph is code even though `code` itself
/// never keys a block.
fn spans_all_in_code(doc: &Document, proj: &Projection, span_range: &Range<usize>) -> bool {
    let spans = &proj.spans[span_range.clone()];
    !spans.is_empty() && spans.iter().all(|s| in_code(doc, s.node))
}

/// Whether a text node has a `code` ancestor before its nearest block.
fn in_code(doc: &Document, node: NodeId) -> bool {
    let mut cur = doc.node(node).parent;
    while let Some(id) = cur {
        if let Some(el) = doc.element(id) {
            if el.name == ElementName::Code {
                return true;
            }
            if is_block(&el.name) {
                return false;
            }
        }
        cur = doc.node(id).parent;
    }
    false
}

fn heading_level(name: &ElementName) -> Option<usize> {
    match name {
        ElementName::H1 => Some(0),
        ElementName::H2 => Some(1),
        ElementName::H3 => Some(2),
        ElementName::H4 => Some(3),
        ElementName::H5 => Some(4),
        ElementName::H6 => Some(5),
        _ => None,
    }
}

/// The block set that opens/closes projection `\n` boundaries — kept
/// identical to `is_block` in the projection (`text/projection.rs`).
pub(crate) fn is_block(name: &ElementName) -> bool {
    matches!(
        name,
        ElementName::P
            | ElementName::Div
            | ElementName::H1
            | ElementName::H2
            | ElementName::H3
            | ElementName::H4
            | ElementName::H5
            | ElementName::H6
            | ElementName::Li
            | ElementName::Blockquote
            | ElementName::Section
            | ElementName::Article
            | ElementName::Tr
            | ElementName::Caption
            | ElementName::Figcaption
            | ElementName::Dt
            | ElementName::Dd
            | ElementName::Pre
    )
}

/// The nearest block-set ancestor of a text node; the document root
/// when text sits directly under `body`.
pub(crate) fn nearest_block(doc: &Document, node: NodeId) -> NodeId {
    let mut cur = doc.node(node).parent;
    while let Some(id) = cur {
        if doc.element(id).is_some_and(|el| is_block(&el.name)) {
            return id;
        }
        cur = doc.node(id).parent;
    }
    doc.root
}

/// Document-order walk mirroring the projector's exclusions
/// (display:none, `rt`), advancing a span pointer on projected text
/// nodes and pinning each hr/img to the next span's start offset.
fn walk_specials(
    styled: &StyledDocument<'_>,
    id: NodeId,
    ptr: &mut usize,
    proj: &Projection,
    out: &mut Vec<(u64, BlockKind)>,
) {
    let doc = styled.doc;
    match &doc.node(id).kind {
        NodeKind::Text(_) => {
            if proj.spans.get(*ptr).is_some_and(|s| s.node == id) {
                *ptr += 1;
            }
        }
        NodeKind::Element(data) => {
            if styled.styles[id.0 as usize].display_none || data.name == ElementName::Rt {
                return;
            }
            match data.name {
                ElementName::Img | ElementName::Image => {
                    if let Some(href) = &data.href {
                        out.push((offset(proj, *ptr), BlockKind::Image {
                            src: href.to_string(),
                        }));
                    }
                }
                ElementName::Hr => out.push((offset(proj, *ptr), BlockKind::Rule)),
                _ => {
                    for &child in &doc.node(id).children {
                        walk_specials(styled, child, ptr, proj, out);
                    }
                }
            }
        }
    }
}

fn offset(proj: &Projection, ptr: usize) -> u64 {
    proj.spans.get(ptr).map_or(proj.char_len, |s| s.char_range.start)
}
