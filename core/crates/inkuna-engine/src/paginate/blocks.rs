//! Block extraction: the styled DOM + canonical projection → a flat
//! block sequence in document order, and per-block shaping into the
//! `ShapedParagraph` glue the line breaker consumes.
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
use crate::layout::{SegmentKind, ShapedParagraph, ShapedSegment};
use crate::shape::{shape_ruby, shape_text, ShapeContext};
use crate::style::{Direction, FontStyle, FontWeight, StyledDocument, TextAlign, WritingMode};
use crate::text::Projection;

use super::pages::{ChapterInput, Metrics};

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
                ElementName::Pre => pre = true,
                _ => {}
            }
            if lang.is_none() {
                lang = el.lang.as_deref().map(str::to_string);
            }
        }
        cur = doc.node(id).parent;
    }
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
    }
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
fn is_block(name: &ElementName) -> bool {
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
fn nearest_block(doc: &Document, node: NodeId) -> NodeId {
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

/// Assembles the paragraph's `ShapedParagraph`: style-uniform groups
/// over its char range (separator chars borrow the previous group's
/// attributes so they merge instead of splitting runs; ruby elements
/// form one atomic segment regardless of interior style changes),
/// each shaped with the resolved style and `Typography` sizes.
pub(super) fn shape_paragraph<'a>(
    input: &ChapterInput<'a>,
    meta: &ParagraphMeta,
    m: &Metrics,
    byte_of_char: &[usize],
) -> ShapedParagraph<'a> {
    let styled = input.styled;
    let proj = input.projection;
    let start_byte = byte_of_char[meta.char_range.start as usize];
    let end_byte = byte_of_char[meta.char_range.end as usize];
    let text = proj.text[start_byte..end_byte].to_string();
    let para_start = meta.char_range.start;
    let para_len = meta.char_range.end - para_start;
    let block_style = styled.styles[meta.node.0 as usize];

    struct Group {
        range: Range<u64>,
        font_style: FontStyle,
        weight: FontWeight,
        ruby: Option<NodeId>,
    }
    let mut groups: Vec<Group> = Vec::new();
    let push = |groups: &mut Vec<Group>, g: Group| {
        if g.range.is_empty() {
            return;
        }
        match groups.last_mut() {
            Some(last)
                if last.range.end == g.range.start
                    && ((last.ruby.is_some() && last.ruby == g.ruby)
                        || (last.font_style == g.font_style
                            && last.weight == g.weight
                            && last.ruby == g.ruby)) =>
            {
                last.range.end = g.range.end;
            }
            _ => groups.push(g),
        }
    };
    let mut cursor = 0u64;
    for span in &proj.spans[meta.span_range.clone()] {
        let s = span.char_range.start - para_start;
        let e = span.char_range.end - para_start;
        if s > cursor {
            let (fs, w) = groups
                .last()
                .map(|g| (g.font_style, g.weight))
                .unwrap_or((block_style.font_style, block_style.font_weight));
            push(&mut groups, Group {
                range: cursor..s,
                font_style: fs,
                weight: w,
                ruby: None,
            });
        }
        let st = styled.styles[span.node.0 as usize];
        push(&mut groups, Group {
            range: s..e,
            font_style: st.font_style,
            weight: st.font_weight,
            ruby: nearest_ruby(styled.doc, span.node),
        });
        cursor = e;
    }
    if cursor < para_len {
        let (fs, w) = groups
            .last()
            .map(|g| (g.font_style, g.weight))
            .unwrap_or((block_style.font_style, block_style.font_weight));
        push(&mut groups, Group {
            range: cursor..para_len,
            font_style: fs,
            weight: w,
            ruby: None,
        });
    }

    // Paragraph-local byte offsets, for slicing group text.
    let mut local_bytes: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    local_bytes.push(text.len());

    let vertical = styled.writing_mode == WritingMode::VerticalRl;
    let (size, letter, word) = match meta.heading {
        Some(l) => (m.heading_size[l], m.letter_heading[l], m.word_heading[l]),
        None => (m.body_size, m.letter_body, m.word_body),
    };
    let family = input.settings.font_family();
    let mut segments = Vec::new();
    for g in groups {
        let slice = &text[local_bytes[g.range.start as usize]..local_bytes[g.range.end as usize]];
        // The reader's bold toggle renders all body text bold; the
        // engine's two-weight model folds publisher emphasis into it.
        let weight = if input.typography.bold_base {
            FontWeight::Bold
        } else {
            g.weight
        };
        let ctx = ShapeContext {
            fonts: input.fonts,
            family,
            font_style: g.font_style,
            font_weight: weight,
            size,
            letter_spacing: letter,
            word_spacing: word,
            lang: meta.lang.as_deref().or(input.lang),
            vertical,
            base_rtl: meta.base_rtl,
        };
        let kind = match g.ruby {
            None => SegmentKind::Text(shape_text(slice, &ctx)),
            Some(ruby_node) => {
                let rt = rt_text(styled.doc, ruby_node);
                let position = styled.styles[ruby_node.0 as usize].ruby_position;
                SegmentKind::Ruby(shape_ruby(
                    slice,
                    &rt,
                    &ctx,
                    input.typography.ruby_scale,
                    position,
                ))
            }
        };
        segments.push(ShapedSegment {
            char_start: g.range.start,
            char_len: g.range.end - g.range.start,
            kind,
        });
    }
    ShapedParagraph {
        text,
        char_range: meta.char_range.clone(),
        segments,
        base_rtl: meta.base_rtl,
        fonts: input.fonts,
    }
}

/// The nearest `ruby` ancestor of a text node, if any.
fn nearest_ruby(doc: &Document, node: NodeId) -> Option<NodeId> {
    let mut cur = doc.node(node).parent;
    while let Some(id) = cur {
        if let Some(el) = doc.element(id) {
            if el.name == ElementName::Ruby {
                return Some(id);
            }
            if is_block(&el.name) {
                return None;
            }
        }
        cur = doc.node(id).parent;
    }
    None
}

/// The collapsed annotation text of a ruby element's `rt` subtrees —
/// gathered from the DOM because the projection excludes `rt`.
fn rt_text(doc: &Document, ruby: NodeId) -> String {
    let mut raw = String::new();
    collect_rt(doc, ruby, false, &mut raw);
    let words: Vec<&str> = raw.split_whitespace().collect();
    words.join(" ")
}

fn collect_rt(doc: &Document, id: NodeId, in_rt: bool, out: &mut String) {
    match &doc.node(id).kind {
        NodeKind::Text(t) => {
            if in_rt {
                out.push_str(t);
            }
        }
        NodeKind::Element(data) => {
            let in_rt = in_rt || data.name == ElementName::Rt;
            for &child in &doc.node(id).children {
                collect_rt(doc, child, in_rt, out);
            }
        }
    }
}
