//! Accessibility blocks: one entry per block-level element with text on
//! the page, in logical reading order. Text comes from the canonical
//! projection slice; ruby readings are appended parenthetically after
//! their base ("漢字（かんじ）"); `lang` is the nearest ancestor
//! `lang`/`xml:lang`.

use std::ops::Range;

use crate::dom::{Document, ElementName, NodeId};
use crate::fixed::Fx;
use crate::paginate::{nearest_block, rt_text, LaidPage};

use super::list::{rect, A11yBlock, A11yRole, DisplayContext};

/// Builds the page's a11y blocks. A block split across pages
/// contributes only its on-page lines' text.
pub(super) fn blocks(
    page: &LaidPage,
    ctx: &DisplayContext<'_>,
    vertical: bool,
) -> Vec<A11yBlock> {
    let doc = ctx.styled.doc;
    let pr = &page.char_range;

    // Consecutive on-page spans sharing a nearest block ancestor form
    // one block, exactly as pagination derives paragraphs.
    let mut groups: Vec<(NodeId, Range<usize>)> = Vec::new();
    for (i, span) in ctx.projection.spans.iter().enumerate() {
        if span.char_range.end <= pr.start || span.char_range.start >= pr.end {
            continue;
        }
        let key = nearest_block(doc, span.node);
        match groups.last_mut() {
            Some((k, r)) if *k == key && r.end == i => r.end = i + 1,
            _ => groups.push((key, i..i + 1)),
        }
    }

    let mut out = Vec::with_capacity(groups.len());
    for (key, span_range) in groups {
        let spans = &ctx.projection.spans[span_range.clone()];
        let start = spans[0].char_range.start.max(pr.start);
        let end = spans[spans.len() - 1].char_range.end.min(pr.end);
        let Some(frame) = block_rect(page, vertical, start..end) else {
            continue;
        };
        let all_linked = spans
            .iter()
            .all(|s| ctx.link_at(s.char_range.start.max(pr.start)).is_some());
        let heading = ancestors(doc, key).any(|el| heading_level(&el.name));
        out.push(A11yBlock {
            text: block_text(ctx, pr, span_range, start..end),
            rect: frame,
            lang: ancestors(doc, key).find_map(|el| el.lang.as_deref().map(str::to_string)),
            is_link: all_linked,
            role: if heading {
                A11yRole::Heading
            } else if all_linked {
                A11yRole::Link
            } else {
                A11yRole::Body
            },
        });
    }
    out
}

/// The block's canonical text slice with ruby readings inserted after
/// each ruby base, in order.
fn block_text(
    ctx: &DisplayContext<'_>,
    pr: &Range<u64>,
    span_range: Range<usize>,
    text_range: Range<u64>,
) -> String {
    let doc = ctx.styled.doc;
    // `(char position, reading)` insertion points, ascending by
    // construction (spans are in document order).
    let mut insertions: Vec<(u64, String)> = Vec::new();
    let mut current: Option<(NodeId, u64)> = None;
    let flush = |cur: &mut Option<(NodeId, u64)>, insertions: &mut Vec<(u64, String)>| {
        if let Some((ruby, at)) = cur.take() {
            let reading = rt_text(doc, ruby);
            if !reading.is_empty() {
                insertions.push((at, reading));
            }
        }
    };
    for span in &ctx.projection.spans[span_range] {
        let ruby = nearest_ruby(doc, span.node);
        match (&mut current, ruby) {
            (Some((cur, at)), Some(r)) if *cur == r => *at = span.char_range.end.min(pr.end),
            _ => {
                flush(&mut current, &mut insertions);
                if let Some(r) = ruby {
                    current = Some((r, span.char_range.end.min(pr.end)));
                }
            }
        }
    }
    flush(&mut current, &mut insertions);

    let mut text = String::new();
    let mut at = text_range.start;
    for (pos, reading) in insertions {
        text.push_str(slice(ctx, at..pos));
        text.push('（');
        text.push_str(&reading);
        text.push('）');
        at = pos;
    }
    text.push_str(slice(ctx, at..text_range.end));
    text
}

/// A canonical char range as its text slice.
fn slice<'a>(ctx: &'a DisplayContext<'_>, r: Range<u64>) -> &'a str {
    &ctx.projection.text[ctx.byte_of_char[r.start as usize]..ctx.byte_of_char[r.end as usize]]
}

/// Union of the tight rects of the lines whose chars overlap `range`.
fn block_rect(page: &LaidPage, vertical: bool, range: Range<u64>) -> Option<super::list::Rect> {
    let mut acc: Option<(Fx, Fx, Fx, Fx)> = None; // x0, y0, x1, y1
    for pl in &page.lines {
        let line = &pl.line;
        if line.char_range.end <= range.start || line.char_range.start >= range.end {
            continue;
        }
        let (x0, y0, x1, y1) = if vertical {
            (
                pl.x - line.descent,
                pl.y,
                pl.x + line.ascent + line.ruby_extent,
                pl.y + line.inline_extent,
            )
        } else {
            (
                pl.x,
                pl.y - line.ascent - line.ruby_extent,
                pl.x + line.inline_extent,
                pl.y + line.descent,
            )
        };
        acc = Some(match acc {
            None => (x0, y0, x1, y1),
            Some((a, b, c, d)) => (a.min(x0), b.min(y0), c.max(x1), d.max(y1)),
        });
    }
    let (x0, y0, x1, y1) = acc?;
    Some(rect(crate::paginate::FxRect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }))
}

/// The node's element ancestors, itself included, innermost first.
fn ancestors<'d>(
    doc: &'d Document,
    node: NodeId,
) -> impl Iterator<Item = &'d crate::dom::ElementData> {
    let mut cur = Some(node);
    std::iter::from_fn(move || {
        while let Some(id) = cur {
            cur = doc.node(id).parent;
            if let Some(el) = doc.element(id) {
                return Some(el);
            }
        }
        None
    })
}

fn heading_level(name: &ElementName) -> bool {
    matches!(
        name,
        ElementName::H1
            | ElementName::H2
            | ElementName::H3
            | ElementName::H4
            | ElementName::H5
            | ElementName::H6
    )
}

/// The nearest `ruby` ancestor below the text node's block, if any.
fn nearest_ruby(doc: &Document, node: NodeId) -> Option<NodeId> {
    let mut cur = doc.node(node).parent;
    while let Some(id) = cur {
        if let Some(el) = doc.element(id) {
            if el.name == ElementName::Ruby {
                return Some(id);
            }
            if crate::paginate::is_block(&el.name) {
                return None;
            }
        }
        cur = doc.node(id).parent;
    }
    None
}
