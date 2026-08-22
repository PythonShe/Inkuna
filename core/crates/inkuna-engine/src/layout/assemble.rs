//! Line assembly: slice the paragraph's shaped runs to one line's char
//! range, reorder to visual order (UBA L2), justify or align, and
//! measure ascent/descent. All arithmetic is `Fx`.

use std::ops::Range;

use crate::fixed::Fx;
use crate::fonts::FontRegistry;
use crate::shape::ShapedRun;
use crate::style::{RubyPosition, TextAlign};

use super::justify;
use super::lines::{Line, PositionedRun, SegmentKind, ShapedParagraph};

/// One visual-order unit of a line: a single shaped run, or one whole
/// ruby pair (base runs + centered annotation runs).
pub(super) struct Item {
    pub level: u8,
    pub base: Vec<WorkRun>,
    pub annotation: Vec<WorkRun>,
    pub is_ruby: bool,
    pub ruby_position: Option<RubyPosition>,
}

/// A run sliced to the line, glyph clusters rebased to canonical
/// projection offsets, trailing-space advances zeroed.
pub(super) struct WorkRun {
    pub run: ShapedRun,
    pub char_range: Range<u64>,
}

/// Per-face vertical metrics in font units: `(font_id, ascender,
/// descender-as-positive, upem)`, cached across the paragraph's lines.
#[derive(Default)]
pub(super) struct MetricsCache(Vec<(u32, i32, i32, u16)>);

/// Builds one line covering paragraph-local chars `local` from the
/// paragraph's segments. `trailing` names the line's trailing space
/// chars (zero advance at the break); `ragged` marks the last line and
/// forced-break lines, which are never justified.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_line(
    p: &ShapedParagraph<'_>,
    chars: &[char],
    local: Range<u64>,
    trailing: Range<u64>,
    width: Fx,
    align: TextAlign,
    ragged: bool,
    cache: &mut MetricsCache,
) -> Line {
    let mut items = slice_items(p, &local, &trailing);
    assign_char_ranges(p.char_range.start, &local, &mut items);
    reorder_visual(&mut items);

    if align == TextAlign::Justify && !ragged {
        let natural = items.iter().map(item_extent).fold(Fx::ZERO, Fx::saturating_add);
        let deficit = width - natural;
        if deficit > Fx::ZERO {
            justify::stretch(&mut items, deficit, chars, p.char_range.start);
        }
    }

    // Flatten in visual order, accumulating inline offsets. Offsets are
    // non-decreasing, not strictly ascending: an item whose glyphs all
    // have zero advance contributes no extent, so the next item repeats
    // its offset.
    let mut runs = Vec::new();
    let mut cursor = Fx::ZERO;
    for item in &items {
        let base = runs_advance(&item.base);
        let annotation = runs_advance(&item.annotation);
        let extent = base.max(annotation);
        let mut at = cursor + half(extent - base);
        for wr in &item.base {
            runs.push(PositionedRun {
                run: wr.run.clone(),
                inline_offset: at,
                char_range: wr.char_range.clone(),
                ruby_position: None,
            });
            at += run_advance(&wr.run);
        }
        let mut at = cursor + half(extent - annotation);
        for wr in &item.annotation {
            runs.push(PositionedRun {
                run: wr.run.clone(),
                inline_offset: at,
                char_range: wr.char_range.clone(),
                ruby_position: item.ruby_position,
            });
            at += run_advance(&wr.run);
        }
        cursor += extent;
    }
    let inline_extent = cursor;

    let shift = align_shift(align, p.base_rtl, width, inline_extent);
    if shift > Fx::ZERO {
        for run in &mut runs {
            run.inline_offset += shift;
        }
    }

    let (ascent, descent, ruby_over_extent, ruby_under_extent) =
        line_metrics(p.fonts, cache, &runs);
    Line {
        runs,
        char_range: p.char_range.start + local.start..p.char_range.start + local.end,
        inline_extent,
        ascent,
        descent,
        ruby_over_extent,
        ruby_under_extent,
    }
}

/// Slices the paragraph's segments to the line's char range, in
/// logical order. Ruby segments are atomic: included whole when their
/// start falls in the range.
fn slice_items(p: &ShapedParagraph<'_>, local: &Range<u64>, trailing: &Range<u64>) -> Vec<Item> {
    let abs = p.char_range.start;
    let mut items = Vec::new();
    for seg in &p.segments {
        let seg_end = seg.char_start + seg.char_len;
        if seg_end <= local.start || seg.char_start >= local.end {
            continue;
        }
        match &seg.kind {
            SegmentKind::Text(runs) => {
                for run in runs {
                    if let Some(wr) = slice_run(run, seg.char_start, local, trailing, abs) {
                        items.push(Item {
                            level: run.bidi_level,
                            base: vec![wr],
                            annotation: Vec::new(),
                            is_ruby: false,
                            ruby_position: None,
                        });
                    }
                }
            }
            SegmentKind::Ruby(ruby) => {
                if seg.char_start < local.start {
                    continue; // atomic: it belongs to the earlier line
                }
                let rebase = |runs: &[ShapedRun], own_clusters: bool| -> Vec<WorkRun> {
                    runs.iter()
                        .map(|run| {
                            let mut run = run.clone();
                            for g in &mut run.glyphs {
                                let at = seg.char_start
                                    + if own_clusters { u64::from(g.cluster) } else { 0 };
                                g.cluster = clamp_u32(abs + at);
                            }
                            WorkRun {
                                run,
                                char_range: 0..0, // assigned below
                            }
                        })
                        .collect()
                };
                let base = rebase(&ruby.base, true);
                let level = base
                    .first()
                    .map(|wr| wr.run.bidi_level)
                    .unwrap_or(u8::from(p.base_rtl));
                items.push(Item {
                    level,
                    base,
                    annotation: rebase(&ruby.annotation, false),
                    is_ruby: true,
                    ruby_position: Some(ruby.position),
                });
            }
        }
    }
    items
}

/// One text run sliced to the line's chars; `None` when no glyph of the
/// run lands on this line.
fn slice_run(
    run: &ShapedRun,
    seg_start: u64,
    local: &Range<u64>,
    trailing: &Range<u64>,
    abs: u64,
) -> Option<WorkRun> {
    let mut glyphs = Vec::new();
    for g in &run.glyphs {
        let at = seg_start + u64::from(g.cluster);
        if at < local.start || at >= local.end {
            continue;
        }
        let mut g = *g;
        if trailing.contains(&at) {
            g.advance = Fx::ZERO;
        }
        g.cluster = clamp_u32(abs + at);
        glyphs.push(g);
    }
    if glyphs.is_empty() {
        return None;
    }
    Some(WorkRun {
        run: ShapedRun {
            glyphs,
            ..run.clone()
        },
        char_range: 0..0, // assigned below
    })
}

/// Canonical char ranges per run by logical adjacency: each base run
/// covers from its first char to the next run's first char (the line
/// end for the last); annotation runs carry their whole ruby's base
/// range (selection maps ruby annotations onto the base).
fn assign_char_ranges(abs: u64, local: &Range<u64>, items: &mut [Item]) {
    let line_end = abs + local.end;
    // (item, run) in logical order with each run's first canonical char.
    let mut mins: Vec<(usize, usize, u64)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        for (r, wr) in item.base.iter().enumerate() {
            let min = wr
                .run
                .glyphs
                .iter()
                .map(|g| u64::from(g.cluster))
                .min()
                .unwrap_or(line_end);
            mins.push((i, r, min));
        }
    }
    for k in 0..mins.len() {
        let (i, r, min) = mins[k];
        let end = mins.get(k + 1).map_or(line_end, |&(_, _, next)| next);
        items[i].base[r].char_range = min..end;
    }
    for item in items.iter_mut().filter(|item| item.is_ruby) {
        let start = item
            .base
            .iter()
            .map(|wr| wr.char_range.start)
            .min()
            .unwrap_or(line_end);
        let end = item
            .base
            .iter()
            .map(|wr| wr.char_range.end)
            .max()
            .unwrap_or(line_end);
        for wr in &mut item.annotation {
            wr.char_range = start..end;
        }
    }
}

/// UBA L2: from the highest level down to the lowest odd level, reverse
/// every maximal contiguous item sequence at or above that level.
fn reorder_visual(items: &mut [Item]) {
    let Some(max) = items.iter().map(|i| i.level).max() else {
        return;
    };
    let Some(lowest_odd) = items.iter().map(|i| i.level).filter(|l| l % 2 == 1).min() else {
        return;
    };
    for level in (lowest_odd..=max).rev() {
        let mut i = 0;
        while i < items.len() {
            if items[i].level >= level {
                let start = i;
                while i < items.len() && items[i].level >= level {
                    i += 1;
                }
                items[start..i].reverse();
            } else {
                i += 1;
            }
        }
    }
}

/// The inline extent one item consumes: its base or its annotation,
/// whichever is wider.
pub(super) fn item_extent(item: &Item) -> Fx {
    runs_advance(&item.base).max(runs_advance(&item.annotation))
}

pub(super) fn runs_advance(runs: &[WorkRun]) -> Fx {
    runs.iter()
        .map(|wr| run_advance(&wr.run))
        .fold(Fx::ZERO, Fx::saturating_add)
}

fn run_advance(run: &ShapedRun) -> Fx {
    run.glyphs
        .iter()
        .map(|g| g.advance)
        .fold(Fx::ZERO, Fx::saturating_add)
}

/// Physical alignment shift for the slack a line leaves. Logical
/// `Start`/`End` resolve against the paragraph base direction;
/// justified lines have no slack, and a justified line whose stretch
/// found no gaps falls back to `Start`.
fn align_shift(align: TextAlign, base_rtl: bool, width: Fx, extent: Fx) -> Fx {
    let slack = width - extent;
    if slack <= Fx::ZERO {
        return Fx::ZERO;
    }
    match align {
        TextAlign::Start | TextAlign::Justify => {
            if base_rtl {
                slack
            } else {
                Fx::ZERO
            }
        }
        TextAlign::End => {
            if base_rtl {
                Fx::ZERO
            } else {
                slack
            }
        }
        TextAlign::Center => half(slack),
    }
}

fn half(v: Fx) -> Fx {
    Fx(v.0 / 2)
}

fn clamp_u32(v: u64) -> u32 {
    v.min(u64::from(u32::MAX)) as u32
}

/// Line ascent/descent from the base runs' faces at their sizes, and
/// ruby extents from annotation runs (their full ascent+descent) on
/// each requested side.
fn line_metrics(
    fonts: &FontRegistry,
    cache: &mut MetricsCache,
    runs: &[PositionedRun],
) -> (Fx, Fx, Fx, Fx) {
    let mut ascent = Fx::ZERO;
    let mut descent = Fx::ZERO;
    let mut ruby_over = Fx::ZERO;
    let mut ruby_under = Fx::ZERO;
    for pr in runs {
        let (asc_units, desc_units, upem) = face_metrics(fonts, cache, pr.run.font_id);
        let asc = Fx::scale_font_units(asc_units, pr.run.size, upem);
        let desc = Fx::scale_font_units(desc_units, pr.run.size, upem);
        if pr.run.style.is_ruby {
            match pr.ruby_position.unwrap_or(RubyPosition::Over) {
                RubyPosition::Over => ruby_over = ruby_over.max(asc + desc),
                RubyPosition::Under => ruby_under = ruby_under.max(asc + desc),
            }
        } else {
            ascent = ascent.max(asc);
            descent = descent.max(desc);
        }
    }
    (ascent, descent, ruby_over, ruby_under)
}

/// Ascender/descender in font units for a face, parsed once per
/// paragraph. Parse failure (impossible past registry load) falls back
/// to the 3/4–1/4 em split rather than panicking.
fn face_metrics(fonts: &FontRegistry, cache: &mut MetricsCache, font_id: u32) -> (i32, i32, u16) {
    if let Some(&(_, asc, desc, upem)) = cache.0.iter().find(|&&(id, ..)| id == font_id) {
        return (asc, desc, upem);
    }
    let face = fonts.face(font_id);
    let (asc, desc) = match rustybuzz::ttf_parser::Face::parse(&face.data, face.collection_index) {
        Ok(parsed) => (
            i32::from(parsed.ascender()),
            i32::from(parsed.descender()).saturating_neg(),
        ),
        Err(_) => (
            i32::from(face.upem) * 3 / 4,
            i32::from(face.upem) / 4,
        ),
    };
    cache.0.push((font_id, asc, desc, face.upem));
    (asc, desc, face.upem)
}
