//! Progressive pagination: blocks → pages, emitted the moment each
//! page fills. All geometry is `Fx` in page coordinates; the only
//! float→fixed conversions happen once, in [`Metrics::new`].

use std::ops::Range;

use crate::error::EngineError;
use crate::fixed::Fx;
use crate::fonts::FontRegistry;
use crate::layout::{break_paragraph, Line, LineOptions, MAX_LINES_PER_PARAGRAPH};
use crate::settings::{LayoutSettings, Typography};
use crate::style::StyledDocument;
use crate::text::Projection;

use super::blocks::{self, BlockKind};
use super::flow::Pager;
use super::images::{self, PlacedImage};

/// Page budget per chapter: layout stops here and the result is a
/// truncated prefix (`char_range`s cover exactly the emitted pages).
pub const MAX_PAGES_PER_CHAPTER: u32 = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FxSize {
    pub width: Fx,
    pub height: Fx,
}

/// A rectangle in page coordinates, used crate-wide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FxRect {
    pub x: Fx,
    pub y: Fx,
    pub w: Fx,
    pub h: Fx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationKind {
    /// An `hr` rule.
    Rule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedDecoration {
    pub rect: FxRect,
    pub kind: DecorationKind,
}

/// One line at its page position. Horizontal writing: `x` is the
/// line's inline origin (inline offsets grow rightward from it), `y`
/// the baseline. Vertical-rl: `x` is the column's baseline (columns
/// advance right-to-left), `y` the inline origin (offsets grow down).
pub struct PlacedLine {
    pub line: Line,
    pub x: Fx,
    pub y: Fx,
}

/// One finished page. `char_range`s of a chapter's pages are ascending
/// and contiguous, partitioning the laid-out projection prefix.
pub struct LaidPage {
    pub index: u32,
    pub char_range: Range<u64>,
    pub lines: Vec<PlacedLine>,
    pub images: Vec<PlacedImage>,
    pub decorations: Vec<PlacedDecoration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterLayoutResult {
    pub page_count: u32,
    /// Chars covered by the emitted pages: the projection's `char_len`,
    /// or the retained prefix length when truncated.
    pub char_len: u64,
    pub truncated: bool,
}

/// Everything one chapter layout consumes. The engine is a pure
/// library: no I/O happens inside `paginate` — image bytes arrive
/// through the `resources` lookup the caller provides.
pub struct ChapterInput<'a> {
    pub styled: &'a StyledDocument<'a>,
    pub projection: &'a Projection,
    pub fonts: &'a FontRegistry,
    pub typography: &'a Typography,
    pub settings: &'a LayoutSettings,
    pub viewport: FxSize,
    pub lang: Option<&'a str>,
    /// Package path of the chapter's own XHTML resource; relative image
    /// hrefs resolve against it (via inkuna-content).
    pub resource_path: &'a str,
    /// Pure lookup from a normalized package-root-relative href to the
    /// resource's bytes. M5's session worker backs this with its
    /// archive reads; `None` degrades to the placeholder box. Must be
    /// deterministic for the layout to be.
    pub resources: &'a dyn Fn(&str) -> Option<Vec<u8>>,
}

/// The chapter's resolved fixed-point numbers — THE float boundary:
/// every `f64` from `Typography`/`LayoutSettings` converts here, once
/// per layout, and everything downstream is `Fx`.
pub(super) struct Metrics {
    pub body_size: Fx,
    pub heading_size: [Fx; 6],
    /// Baseline-to-baseline advance for body lines.
    pub line_advance: Fx,
    /// Heading line advance: `line_height × heading_scale[level]`, so
    /// scaled headings keep their proportional leading.
    pub heading_advance: [Fx; 6],
    pub para_spacing: Fx,
    pub indent: Fx,
    pub letter_body: Fx,
    pub letter_heading: [Fx; 6],
    pub word_body: Fx,
    pub word_heading: [Fx; 6],
    /// Blockquote inline inset per nesting level: 1.5 em of body size.
    pub quote_inset: Fx,
    /// Page margins on all four sides (the shells had no vertical inset
    /// beyond `reading_margins`, so it applies to every side).
    pub margin: Fx,
}

impl Metrics {
    /// `settings` must already be clamped.
    pub fn new(t: &Typography, settings: &LayoutSettings) -> Metrics {
        let scale = |i: usize| t.heading_scale[i];
        Metrics {
            body_size: Fx::from_pt(t.font_size),
            heading_size: std::array::from_fn(|i| Fx::from_pt(t.font_size * scale(i))),
            line_advance: Fx::from_pt(t.line_height),
            heading_advance: std::array::from_fn(|i| Fx::from_pt(t.line_height * scale(i))),
            para_spacing: Fx::from_pt(t.paragraph_spacing),
            indent: Fx::from_pt(t.paragraph_indent),
            letter_body: Fx::from_pt(t.font_size * settings.letter_spacing),
            letter_heading: std::array::from_fn(|i| {
                Fx::from_pt(t.font_size * scale(i) * settings.letter_spacing)
            }),
            word_body: Fx::from_pt(t.font_size * settings.word_spacing),
            word_heading: std::array::from_fn(|i| {
                Fx::from_pt(t.font_size * scale(i) * settings.word_spacing)
            }),
            quote_inset: Fx::from_pt(t.font_size * 1.5),
            margin: Fx::from_pt(f64::from(settings.reading_margins)),
        }
    }
}

/// Lays the chapter out, calling `emit` the moment each page is full —
/// long before the chapter finishes. Page breaks happen only at line
/// boundaries; widow/orphan (min 2 lines on both sides of a paragraph
/// split) and heading keep (heading + 2 lines of the next block) are
/// honored by pushing breaks earlier. Truncation — the projection's
/// own flag, a paragraph tripping the line cap, or the page cap — is
/// the degradation path; `paginate` itself never fails on content.
pub fn paginate(
    input: &ChapterInput<'_>,
    emit: &mut dyn FnMut(LaidPage),
) -> Result<ChapterLayoutResult, EngineError> {
    let settings = input.settings.clone().clamped();
    let m = Metrics::new(input.typography, &settings);
    let mut pager = Pager::new(input, &m);
    let blocks = blocks::collect_blocks(input);

    // Char → byte offsets of the projection text, for paragraph slicing.
    let mut byte_of_char: Vec<usize> =
        input.projection.text.char_indices().map(|(b, _)| b).collect();
    byte_of_char.push(input.projection.text.len());

    // Broken lines per block, computed lazily (one-block lookahead for
    // heading keep) so emission stays progressive.
    let mut cache: Vec<Option<Vec<Line>>> = (0..blocks.len()).map(|_| None).collect();
    let mut truncated = input.projection.truncated;

    for i in 0..blocks.len() {
        if pager.stopped() {
            break;
        }
        match &blocks[i] {
            BlockKind::Paragraph(meta) => {
                ensure_lines(&mut cache, i, &blocks, input, &m, &byte_of_char, &pager);
                if meta.heading.is_some() {
                    let heading_extent = block_extent(&m, &blocks, i, &cache, usize::MAX);
                    let next_two = match blocks.get(i + 1) {
                        Some(BlockKind::Paragraph(_)) => {
                            ensure_lines(&mut cache, i + 1, &blocks, input, &m, &byte_of_char, &pager);
                            m.para_spacing + block_extent(&m, &blocks, i + 1, &cache, 2)
                        }
                        _ => Fx::ZERO,
                    };
                    pager.require(m.para_spacing + heading_extent + next_two, emit);
                }
                let lines = cache[i].take().unwrap_or_default();
                let para_truncated = lines
                    .last()
                    .is_some_and(|l| l.char_range.end < meta.char_range.end);
                let adv = match meta.heading {
                    Some(l) => m.heading_advance[l],
                    None => m.line_advance,
                };
                let inset = m.quote_inset.mul_ratio(meta.quote_depth as i32, 1);
                pager.place_paragraph(lines, adv, inset, m.para_spacing, m.indent, emit);
                if para_truncated {
                    // Prefix semantics: a line-capped paragraph ends
                    // the laid-out prefix.
                    truncated = true;
                    break;
                }
            }
            BlockKind::Rule => pager.place_rule(&m, emit),
            BlockKind::Image { src } => {
                let measured =
                    images::measure_image(src, input.resource_path, input.resources);
                pager.place_image(measured, &m, emit);
            }
        }
    }

    let (page_count, char_len, capped) = pager.finish(emit);
    Ok(ChapterLayoutResult {
        page_count,
        char_len,
        truncated: truncated || (capped && char_len < input.projection.char_len),
    })
}

/// Breaks block `i`'s paragraph into lines if not already cached.
fn ensure_lines(
    cache: &mut [Option<Vec<Line>>],
    i: usize,
    blocks: &[BlockKind],
    input: &ChapterInput<'_>,
    m: &Metrics,
    byte_of_char: &[usize],
    pager: &Pager,
) {
    if cache[i].is_some() {
        return;
    }
    let BlockKind::Paragraph(meta) = &blocks[i] else {
        return;
    };
    let inset = m.quote_inset.mul_ratio(meta.quote_depth as i32, 1);
    let width = (pager.inline_total() - inset - inset).max(Fx(64));
    let shaped = blocks::shape_paragraph(input, meta, m, byte_of_char);
    let opts = LineOptions {
        justify: meta.align,
        last_line_ragged: true,
        max_lines: MAX_LINES_PER_PARAGRAPH,
    };
    cache[i] = Some(break_paragraph(&shaped, width, &opts));
}

/// Block extent of the first `take` lines of block `i`'s cached lines.
fn block_extent(
    m: &Metrics,
    blocks: &[BlockKind],
    i: usize,
    cache: &[Option<Vec<Line>>],
    take: usize,
) -> Fx {
    let BlockKind::Paragraph(meta) = &blocks[i] else {
        return Fx::ZERO;
    };
    let adv = match meta.heading {
        Some(l) => m.heading_advance[l],
        None => m.line_advance,
    };
    let Some(Some(lines)) = cache.get(i) else {
        return Fx::ZERO;
    };
    lines
        .iter()
        .take(take)
        .map(|l| adv + l.ruby_extent)
        .fold(Fx::ZERO, Fx::saturating_add)
}
