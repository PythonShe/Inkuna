use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::fixed::Fx;
use crate::fonts::FontRegistry;
use crate::settings::FontFamily;
use crate::shape::{shape_text, ShapeContext};
use crate::style::{FontStyle, FontWeight, TextAlign};

use super::{break_paragraph, Line, LineOptions, SegmentKind, ShapedParagraph, ShapedSegment};

fn registry() -> &'static FontRegistry {
    static REG: OnceLock<Arc<FontRegistry>> = OnceLock::new();
    REG.get_or_init(|| {
        let dir = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../assets/fonts"
        ));
        FontRegistry::load(dir).expect("repo font set must load")
    })
}

fn ctx(fonts: &FontRegistry, base_rtl: bool) -> ShapeContext<'_> {
    ShapeContext {
        fonts,
        family: FontFamily::NotoSerif,
        font_style: FontStyle::Normal,
        font_weight: FontWeight::Normal,
        size: Fx::from_pt(16.0),
        letter_spacing: Fx::ZERO,
        word_spacing: Fx::ZERO,
        lang: None,
        vertical: false,
        base_rtl,
    }
}

/// A one-segment paragraph shaped from `text`, at canonical offset
/// `char_start` (nonzero exercises the projection-offset rebasing).
fn paragraph<'a>(
    text: &str,
    base_rtl: bool,
    char_start: u64,
    fonts: &'a FontRegistry,
) -> ShapedParagraph<'a> {
    let runs = shape_text(text, &ctx(fonts, base_rtl));
    let char_len = text.chars().count() as u64;
    ShapedParagraph {
        text: text.to_string(),
        char_range: char_start..char_start + char_len,
        segments: vec![ShapedSegment {
            char_start: 0,
            char_len,
            kind: SegmentKind::Text(runs),
        }],
        base_rtl,
        fonts,
    }
}

fn opts(justify: TextAlign) -> LineOptions {
    LineOptions {
        justify,
        last_line_ragged: true,
        max_lines: super::MAX_LINES_PER_PARAGRAPH,
    }
}

/// Advance of the first glyph of `text` shaped alone.
fn one_char_advance(text: &str, fonts: &FontRegistry) -> Fx {
    let runs = shape_text(text, &ctx(fonts, false));
    runs[0].glyphs[0].advance
}

fn total_advance(text: &str, fonts: &FontRegistry) -> Fx {
    shape_text(text, &ctx(fonts, false))
        .iter()
        .flat_map(|r| r.glyphs.iter())
        .fold(Fx::ZERO, |acc, g| acc + g.advance)
}

/// The char at a line's start, via its canonical char_range.
fn first_char(line: &Line, text: &str, para_start: u64) -> char {
    let local = (line.char_range.start - para_start) as usize;
    text.chars().nth(local).expect("line start inside paragraph")
}

#[test]
fn kinsoku_no_leading_close_bracket() {
    let fonts = registry();
    let text = "ああああ」いいい";
    let w = one_char_advance("あ", fonts);
    // Four full-width chars fit; the naive break before 」 (char 4) is
    // forbidden by UAX-14, so the break moves earlier.
    let width = w.mul_ratio(9, 2); // 4.5 chars
    let p = paragraph(text, false, 0, fonts);
    let lines = break_paragraph(&p, width, &opts(TextAlign::Start));
    assert!(lines.len() >= 2, "got {} lines", lines.len());
    for line in &lines {
        assert_ne!(
            first_char(line, text, 0),
            '」',
            "」 must never start a line"
        );
    }
    // The bracket stayed glued to the char before it: the first line is
    // shorter than the four chars that would naively fit.
    assert!(lines[0].char_range.end < 4);
}

#[test]
fn justify_latin_stretches_spaces() {
    let fonts = registry();
    let text = "aaa bbb ccc ddd";
    let width = total_advance(text, fonts).mul_ratio(2, 3);
    let p = paragraph(text, false, 0, fonts);
    let lines = break_paragraph(&p, width, &opts(TextAlign::Justify));
    assert!(lines.len() >= 2);
    assert_eq!(lines[0].inline_extent, width, "line 0 justified exactly");
    // Some interior space advance grew beyond its shaped advance.
    let raw_space = {
        let runs = shape_text(text, &ctx(fonts, false));
        runs
            .iter()
            .flat_map(|r| r.glyphs.iter())
            .find(|g| text.chars().nth(g.cluster as usize) == Some(' '))
            .expect("space glyph")
            .advance
    };
    let chars: Vec<char> = text.chars().collect();
    let grew = lines[0].runs.iter().flat_map(|pr| pr.run.glyphs.iter()).any(|g| {
        chars.get(g.cluster as usize) == Some(&' ') && g.advance > raw_space
    });
    assert!(grew, "an interior space stretched");
    // The last line stays ragged.
    let last = lines.last().expect("lines");
    assert!(last.inline_extent < width);
}

#[test]
fn justify_cjk_inter_character() {
    let fonts = registry();
    let text = "春眠不覚暁処処聞啼鳥夜来風雨";
    let w = one_char_advance("春", fonts);
    let width = w.mul_ratio(21, 2); // 10.5 chars: 10 fit, half a char of deficit
    let p = paragraph(text, false, 0, fonts);
    let lines = break_paragraph(&p, width, &opts(TextAlign::Justify));
    assert!(lines.len() >= 2);
    assert_eq!(lines[0].inline_extent, width, "line 0 justified exactly");
    // Per-gap growth is uniform to within one 1/64 unit; the line's
    // last glyph leads no gap and keeps its natural advance.
    let raw: Vec<Fx> = shape_text(text, &ctx(fonts, false))
        .iter()
        .flat_map(|r| r.glyphs.iter().map(|g| g.advance))
        .collect();
    let line0: Vec<(u32, Fx)> = lines[0]
        .runs
        .iter()
        .flat_map(|pr| pr.run.glyphs.iter().map(|g| (g.cluster, g.advance)))
        .collect();
    let deltas: Vec<i32> = line0
        .iter()
        .take(line0.len() - 1)
        .map(|&(cluster, adv)| (adv - raw[cluster as usize]).0)
        .collect();
    assert!(!deltas.is_empty());
    let min = deltas.iter().min().expect("deltas");
    let max = deltas.iter().max().expect("deltas");
    assert!(*min > 0, "every gap stretched");
    assert!(max - min <= 1, "uniform to one 1/64 unit: {deltas:?}");
    let last = line0.last().expect("glyphs");
    assert_eq!(last.1, raw[last.0 as usize], "last glyph leads no gap");
}

#[test]
fn remainder_distribution_deterministic() {
    let fonts = registry();
    let text = "春眠不覚暁処処聞啼鳥夜来風雨声天地玄黄宇宙洪荒";
    let w = one_char_advance("春", fonts);
    let width = w.mul_ratio(31, 4);
    let offsets = |lines: &[Line]| -> Vec<Vec<i32>> {
        lines
            .iter()
            .map(|l| l.runs.iter().map(|r| r.inline_offset.0).collect())
            .collect()
    };
    let a = break_paragraph(&paragraph(text, false, 0, fonts), width, &opts(TextAlign::Justify));
    let b = break_paragraph(&paragraph(text, false, 0, fonts), width, &opts(TextAlign::Justify));
    assert_eq!(offsets(&a), offsets(&b), "identical inline_offset vectors");
    let advances = |lines: &[Line]| -> Vec<i32> {
        lines
            .iter()
            .flat_map(|l| l.runs.iter())
            .flat_map(|r| r.run.glyphs.iter().map(|g| g.advance.0))
            .collect()
    };
    assert_eq!(advances(&a), advances(&b));
}

#[test]
fn bidi_visual_reorder() {
    let fonts = registry();
    let text = "אבג abc דהו";
    let p = paragraph(text, true, 0, fonts);
    let lines = break_paragraph(&p, Fx::from_pt(500.0), &opts(TextAlign::Start));
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert!(line.runs.len() >= 3, "got {} runs", line.runs.len());
    // Offsets are assigned in strictly ascending visual order.
    let offsets: Vec<Fx> = line.runs.iter().map(|r| r.inline_offset).collect();
    for pair in offsets.windows(2) {
        assert!(pair[0] < pair[1], "offsets ascend: {offsets:?}");
    }
    // Visual order differs from logical: in an RTL paragraph the last
    // logical Hebrew run renders leftmost, the first rightmost.
    let starts: Vec<u64> = line.runs.iter().map(|r| r.char_range.start).collect();
    let mut logical = starts.clone();
    logical.sort_unstable();
    assert_ne!(starts, logical, "visual differs from logical");
    let first = line.runs.first().expect("runs");
    let last = line.runs.last().expect("runs");
    assert!(first.char_range.start > last.char_range.start);
}

#[test]
fn char_ranges_partition_paragraph() {
    let fonts = registry();
    let text = "月光洒在窗台上，屋里一片寂静。他放下手中的书，望向远处的群山。";
    let start = 1_000u64;
    let w = one_char_advance("月", fonts);
    let p = paragraph(text, false, start, fonts);
    let lines = break_paragraph(&p, w.mul_ratio(13, 2), &opts(TextAlign::Justify));
    assert!(lines.len() >= 3, "got {} lines", lines.len());
    let mut cursor = p.char_range.start;
    for line in &lines {
        assert_eq!(line.char_range.start, cursor, "no gap, no overlap");
        assert!(line.char_range.end > line.char_range.start);
        cursor = line.char_range.end;
    }
    assert_eq!(cursor, p.char_range.end, "lines cover the paragraph");
}
