//! Turns itemized text into positioned glyph runs via rustybuzz, with
//! the explicit fallback chain: reading face → CJK face → symbols →
//! the reading face's `.notdef`. Text is never dropped. Missing-glyph
//! detection is cluster-granular, so ligatures never straddle fonts.
//!
//! Determinism: rustybuzz is pure Rust over fixed bytes, and no path
//! that orders output iterates a HashMap.

use crate::fixed::Fx;
use crate::fonts::FontRegistry;
use crate::settings::FontFamily;
use crate::style::{FontStyle, FontWeight};

use super::itemize::{itemize, Item};

/// One positioned glyph. `cluster` is the char offset into the shaped
/// text's chars (ruby annotations remap it onto their base).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    pub glyph_id: u16,
    pub cluster: u32,
    /// Advance along the line's inline axis.
    pub advance: Fx,
    pub offset_x: Fx,
    pub offset_y: Fx,
}

/// How the shells draw a run in vertical writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOrientation {
    Upright,
    SidewaysRotated,
}

/// The style facts a run carries into the display list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunStyle {
    pub font_style: FontStyle,
    pub font_weight: FontWeight,
    pub is_ruby: bool,
}

/// A maximal single-font, single-level glyph sequence. Glyphs are in
/// rustybuzz's output order (visual order within the run for RTL);
/// runs themselves are in logical order — M4 reorders by `bidi_level`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapedRun {
    pub font_id: u32,
    pub size: Fx,
    pub glyphs: Vec<Glyph>,
    pub bidi_level: u8,
    pub orientation: RunOrientation,
    pub style: RunStyle,
}

/// Everything shaping needs; spacing values are already resolved to
/// `Fx` by the caller (em values against `size` at call time).
#[derive(Clone, Copy)]
pub struct ShapeContext<'a> {
    pub fonts: &'a FontRegistry,
    pub family: FontFamily,
    pub font_style: FontStyle,
    pub font_weight: FontWeight,
    pub size: Fx,
    pub letter_spacing: Fx,
    pub word_spacing: Fx,
    pub lang: Option<&'a str>,
    pub vertical: bool,
    pub base_rtl: bool,
}

/// The fallback chain, in order. `Notdef` re-shapes with the reading
/// face and accepts glyph 0 — the terminal stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Reading,
    Cjk,
    Symbols,
    Notdef,
}

impl Stage {
    fn next(self) -> Option<Stage> {
        match self {
            Stage::Reading => Some(Stage::Cjk),
            Stage::Cjk => Some(Stage::Symbols),
            Stage::Symbols => Some(Stage::Notdef),
            Stage::Notdef => None,
        }
    }

    fn font_id(self, ctx: &ShapeContext) -> u32 {
        match self {
            Stage::Reading | Stage::Notdef => {
                ctx.fonts
                    .select(ctx.family, ctx.font_style, ctx.font_weight)
            }
            Stage::Cjk => ctx.fonts.cjk(
                ctx.lang,
                ctx.family == FontFamily::NotoSerif,
                ctx.font_weight,
            ),
            Stage::Symbols => ctx.fonts.symbols(),
        }
    }
}

/// Raw rustybuzz output, still in font units.
#[derive(Debug, Clone, Copy)]
pub(super) struct RawGlyph {
    pub glyph_id: u32,
    pub cluster: u32,
    pub x_advance: i32,
    pub y_advance: i32,
    pub x_offset: i32,
    pub y_offset: i32,
}

/// Shape a paragraph's text into runs. Empty input yields an empty vec.
pub fn shape_text(text: &str, ctx: &ShapeContext) -> Vec<ShapedRun> {
    let chars: Vec<char> = text.chars().collect();
    let mut runs = Vec::new();
    let mut start_char = 0usize;
    for item in itemize(text, ctx.base_rtl) {
        let slice = &text[item.range.clone()];
        let n_chars = slice.chars().count();
        shape_slice(
            slice,
            start_char,
            &item,
            Stage::Reading,
            ctx,
            &chars,
            &mut runs,
        );
        start_char += n_chars;
    }
    runs
}

/// Shape one slice at one chain stage, splitting missing clusters off
/// to the next stage. Appends runs in logical order: groups are walked
/// in ascending cluster order and recursion preserves it.
fn shape_slice(
    slice: &str,
    start_char: usize,
    item: &Item,
    stage: Stage,
    ctx: &ShapeContext,
    chars: &[char],
    runs: &mut Vec<ShapedRun>,
) {
    if slice.is_empty() {
        return;
    }
    let font_id = stage.font_id(ctx);
    let (raw, orientation) = super::vertical::shape_once(slice, start_char, font_id, item, ctx);
    if raw.is_empty() {
        return;
    }

    // Cluster values present, with a missing flag when any glyph of
    // the cluster is .notdef; sorted ascending = logical order.
    let mut clusters: Vec<(u32, bool)> = Vec::new();
    for g in &raw {
        match clusters.iter_mut().find(|(c, _)| *c == g.cluster) {
            Some((_, missing)) => *missing |= g.glyph_id == 0,
            None => clusters.push((g.cluster, g.glyph_id == 0)),
        }
    }
    clusters.sort_unstable_by_key(|(c, _)| *c);

    let end_char = (start_char + slice.chars().count()) as u32;
    let terminal = stage.next().is_none();
    if terminal || clusters.iter().all(|(_, missing)| !missing) {
        runs.push(build_run(&raw, font_id, item, orientation, ctx, chars));
        return;
    }

    // Byte offset of each of the slice's chars, plus the end.
    let mut char_bytes: Vec<usize> = slice.char_indices().map(|(b, _)| b).collect();
    char_bytes.push(slice.len());

    // Maximal same-flag groups over the logical cluster order.
    let mut idx = 0;
    while idx < clusters.len() {
        let missing = clusters[idx].1;
        let group_start = clusters[idx].0;
        let mut end_idx = idx;
        while end_idx + 1 < clusters.len() && clusters[end_idx + 1].1 == missing {
            end_idx += 1;
        }
        let group_end = clusters.get(end_idx + 1).map_or(end_char, |(c, _)| *c);
        if missing {
            let rel_start = char_bytes[(group_start as usize) - start_char];
            let rel_end = char_bytes[(group_end as usize) - start_char];
            if let Some(next) = stage.next() {
                shape_slice(
                    &slice[rel_start..rel_end],
                    group_start as usize,
                    item,
                    next,
                    ctx,
                    chars,
                    runs,
                );
            }
        } else {
            let group: Vec<RawGlyph> = raw
                .iter()
                .filter(|g| g.cluster >= group_start && g.cluster < group_end)
                .copied()
                .collect();
            runs.push(build_run(&group, font_id, item, orientation, ctx, chars));
        }
        idx = end_idx + 1;
    }
}

/// Scale raw glyphs to `Fx` and apply spacing: letter spacing adds to
/// every cluster's closing advance, word spacing to space clusters.
fn build_run(
    raw: &[RawGlyph],
    font_id: u32,
    item: &Item,
    orientation: RunOrientation,
    ctx: &ShapeContext,
    chars: &[char],
) -> ShapedRun {
    let upem = ctx.fonts.face(font_id).upem;
    let along_block = ctx.vertical && orientation == RunOrientation::Upright;
    let mut glyphs = Vec::with_capacity(raw.len());
    for (i, g) in raw.iter().enumerate() {
        // Vertical upright advances run along the block axis —
        // rustybuzz reports them as negative y; the line's inline
        // advance is their magnitude.
        let advance_units = if along_block {
            g.y_advance.saturating_abs()
        } else {
            g.x_advance
        };
        let mut advance = Fx::scale_font_units(advance_units, ctx.size, upem);
        let closes_cluster = raw.get(i + 1).is_none_or(|n| n.cluster != g.cluster);
        if closes_cluster {
            advance = advance + ctx.letter_spacing;
            if chars
                .get(g.cluster as usize)
                .is_some_and(|c| *c == ' ' || *c == '\u{00A0}')
            {
                advance = advance + ctx.word_spacing;
            }
        }
        glyphs.push(Glyph {
            glyph_id: g.glyph_id as u16,
            cluster: g.cluster,
            advance,
            offset_x: Fx::scale_font_units(g.x_offset, ctx.size, upem),
            offset_y: Fx::scale_font_units(g.y_offset, ctx.size, upem),
        });
    }
    ShapedRun {
        font_id,
        size: ctx.size,
        glyphs,
        bidi_level: item.bidi_level,
        orientation,
        style: RunStyle {
            font_style: ctx.font_style,
            font_weight: ctx.font_weight,
            is_ruby: false,
        },
    }
}
