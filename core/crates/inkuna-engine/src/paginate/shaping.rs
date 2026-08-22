//! Per-block shaping: one paragraph block's spans assembled into the
//! `ShapedParagraph` glue the line breaker consumes — style-uniform
//! groups, ruby gathering, and the shape calls themselves.

use std::ops::Range;

use crate::dom::{Document, ElementName, NodeId, NodeKind};
use crate::layout::{SegmentKind, ShapedParagraph, ShapedSegment};
use crate::shape::{shape_ruby, shape_text, ShapeContext};
use crate::style::{FontStyle, FontWeight, WritingMode};

use super::blocks::{is_block, ParagraphMeta};
use super::pages::{ChapterInput, Metrics};

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
