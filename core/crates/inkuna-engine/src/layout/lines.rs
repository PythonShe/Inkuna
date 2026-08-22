//! Greedy UAX-14 line breaking over one shaped paragraph.
//!
//! The paragraph text is measured as a sequence of atoms — one glyph
//! cluster, or one whole ruby pair — and break candidates are the
//! UAX-14 opportunities that land on atom boundaries (so ligatures and
//! ruby stay unbreakable). A single unbreakable stretch wider than the
//! line overflows rather than force-breaking; the page box clips it.

use std::ops::Range;

use unicode_linebreak::{linebreaks, BreakOpportunity};

use crate::fixed::Fx;
use crate::fonts::FontRegistry;
use crate::shape::{RubyRun, ShapedRun};
use crate::style::{RubyPosition, TextAlign};

use super::assemble::{self, MetricsCache};

#[cfg(test)]
#[path = "lines_tests.rs"]
mod tests;

/// Line-count budget guard per paragraph: excess lines are dropped and
/// the caller detects the prefix (the last line ends short of the
/// paragraph range) to mark the layout truncated.
pub const MAX_LINES_PER_PARAGRAPH: u32 = 8_192;

/// One paragraph, shaped and ready to break — the glue type the block
/// walk (paginate) assembles for [`break_paragraph`].
pub struct ShapedParagraph<'a> {
    /// The paragraph's slice of the canonical projection text. May end
    /// with the block-separator `\n` and may carry `<br>` newlines
    /// inside (mandatory breaks); neither emits glyphs.
    pub text: String,
    /// Canonical projection char range `text` covers.
    pub char_range: Range<u64>,
    /// Shaping segments in logical order, covering `text` contiguously.
    pub segments: Vec<ShapedSegment>,
    /// Paragraph base direction, from the resolved style.
    pub base_rtl: bool,
    /// For ascent/descent metrics of the runs' faces.
    pub fonts: &'a FontRegistry,
}

/// One shaping call's worth of the paragraph: glyph clusters are char
/// offsets local to the segment's own text (its slice of the paragraph).
pub struct ShapedSegment {
    /// Paragraph-local char offset of the segment's first char.
    pub char_start: u64,
    /// Chars of the paragraph text this segment covers.
    pub char_len: u64,
    pub kind: SegmentKind,
}

pub enum SegmentKind {
    /// Plain shaped runs, in logical order.
    Text(Vec<ShapedRun>),
    /// One atomic ruby pair; never split across lines.
    Ruby(RubyRun),
}

/// How [`break_paragraph`] aligns and caps the paragraph's lines.
/// The final line (and lines ending in a forced break) is never
/// justified — unconditionally.
pub struct LineOptions {
    /// The paragraph's resolved alignment; `Justify` stretches every
    /// line but the last (and lines ending in a forced break).
    pub justify: TextAlign,
    /// First-line indent: the paragraph's first line is broken and
    /// justified against `width - first_line_indent`, so the caller's
    /// inline shift of that line never overflows the measure. Zero for
    /// non-indented paragraphs.
    pub first_line_indent: Fx,
    /// Line cap, clamped to [`MAX_LINES_PER_PARAGRAPH`].
    pub max_lines: u32,
}

/// One run placed on a line, in visual order. Glyph clusters have been
/// rebased to canonical projection char offsets. Offsets are
/// non-decreasing in visual order — a run whose glyphs all carry zero
/// advance (e.g. only trailing spaces) repeats the previous offset
/// rather than strictly exceeding it. Ruby annotation runs
/// (`run.style.is_ruby`) are centered over their base and exempt from
/// the ordering; they carry the base's char range and requested side.
pub struct PositionedRun {
    pub run: ShapedRun,
    /// Inline-axis offset from the line's start (alignment included).
    pub inline_offset: Fx,
    /// Canonical projection char range this run covers.
    pub char_range: Range<u64>,
    /// `Some` only for ruby annotation runs, preserving the computed
    /// side through line assembly into display emission.
    pub ruby_position: Option<RubyPosition>,
}

/// One laid-out line. `char_range` is canonical projection offsets;
/// concatenated line ranges partition the paragraph range exactly.
pub struct Line {
    pub runs: Vec<PositionedRun>,
    pub char_range: Range<u64>,
    /// Used inline extent (width horizontal, height vertical) — the
    /// justified width for justified lines, the natural width otherwise.
    pub inline_extent: Fx,
    pub ascent: Fx,
    pub descent: Fx,
    /// Extra block extent ruby annotations need on the over side.
    pub ruby_over_extent: Fx,
    /// Extra block extent ruby annotations need on the under side.
    pub ruby_under_extent: Fx,
}

/// The smallest unbreakable measure unit: one glyph cluster or one
/// whole ruby pair. Atoms cover the paragraph's chars contiguously —
/// glyphless chars (`\n`, ignorables, ligature interiors) fold into the
/// preceding atom's range.
struct Atom {
    /// Paragraph-local char range.
    start: u64,
    end: u64,
    advance: Fx,
    is_space: bool,
}

/// Breaks one shaped paragraph into lines at `width`. Infallible; a
/// dropped tail (line cap) is detectable by the last line's range.
pub fn break_paragraph(p: &ShapedParagraph<'_>, width: Fx, opts: &LineOptions) -> Vec<Line> {
    let chars: Vec<char> = p.text.chars().collect();
    let n_chars = chars.len() as u64;
    let atoms = build_atoms(p, &chars, n_chars);
    if atoms.is_empty() {
        // Nothing shaped (an all-ignorable paragraph): one empty line
        // so the char range still lands on exactly one page.
        return vec![Line {
            runs: Vec::new(),
            char_range: p.char_range.clone(),
            inline_extent: Fx::ZERO,
            ascent: Fx::ZERO,
            descent: Fx::ZERO,
            ruby_over_extent: Fx::ZERO,
            ruby_under_extent: Fx::ZERO,
        }];
    }
    let cands = candidates(&p.text, &atoms, n_chars);
    let prefix = prefix_sums(&atoms);
    let cap = opts.max_lines.min(MAX_LINES_PER_PARAGRAPH) as usize;
    let mut cache = MetricsCache::default();
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut ci = 0usize;
    while start < atoms.len() && lines.len() < cap {
        // The first line is measured against the indent-reduced width;
        // the guard keeps a degenerate indent from collapsing it.
        let line_width = if lines.is_empty() {
            (width - opts.first_line_indent).max(Fx(64))
        } else {
            width
        };
        while ci < cands.len() && cands[ci].0 <= start {
            ci += 1;
        }
        // Widest candidate that fits; a mandatory break is taken as
        // soon as it fits; if not even the first candidate fits, take
        // it anyway (overflow — the page box clips).
        let mut chosen: Option<(usize, bool)> = None;
        for &(c, mandatory) in &cands[ci..] {
            if measure(&atoms, &prefix, start, c) <= line_width {
                chosen = Some((c, mandatory));
                if mandatory {
                    break;
                }
            } else {
                if chosen.is_none() {
                    chosen = Some((c, mandatory));
                }
                break;
            }
        }
        // The end-of-text candidate always exists, so `chosen` is Some;
        // the fallback keeps this panic-free regardless.
        let (end, forced) = chosen.unwrap_or((atoms.len(), true));
        let is_last = end == atoms.len();
        let lo = atoms[start].start;
        let hi = atom_end(&atoms, end, n_chars);
        let trailing = trailing_spaces(&atoms, start, end);
        lines.push(assemble::build_line(
            p,
            &chars,
            lo..hi,
            trailing,
            line_width,
            opts.justify,
            is_last || forced,
            &mut cache,
        ));
        start = end;
    }
    lines
}

/// The paragraph-local char offset just past atom index `end`.
fn atom_end(atoms: &[Atom], end: usize, n_chars: u64) -> u64 {
    match atoms.get(end) {
        Some(atom) => atom.start,
        None => n_chars,
    }
}

/// The char range of the trailing space atoms of `[start, end)` —
/// their glyphs render at zero advance at a line break.
fn trailing_spaces(atoms: &[Atom], start: usize, end: usize) -> Range<u64> {
    let mut e = end;
    while e > start && atoms[e - 1].is_space {
        e -= 1;
    }
    if e == end {
        return 0..0;
    }
    atoms[e].start..atoms[end - 1].end
}

/// Inline extent of atoms `[a, b)` excluding trailing spaces.
fn measure(atoms: &[Atom], prefix: &[i64], a: usize, b: usize) -> Fx {
    let mut e = b;
    while e > a && atoms[e - 1].is_space {
        e -= 1;
    }
    let units = prefix[e] - prefix[a];
    if units >= i64::from(i32::MAX) {
        Fx(i32::MAX)
    } else {
        Fx(units as i32)
    }
}

fn prefix_sums(atoms: &[Atom]) -> Vec<i64> {
    let mut prefix = Vec::with_capacity(atoms.len() + 1);
    let mut acc = 0i64;
    prefix.push(acc);
    for atom in atoms {
        acc += i64::from(atom.advance.0);
        prefix.push(acc);
    }
    prefix
}

/// Atoms in ascending char order, covering `0..n_chars` contiguously.
fn build_atoms(p: &ShapedParagraph<'_>, chars: &[char], n_chars: u64) -> Vec<Atom> {
    let mut atoms: Vec<Atom> = Vec::new();
    for seg in &p.segments {
        match &seg.kind {
            SegmentKind::Text(runs) => {
                // Per-cluster advance sums; clusters are unique across
                // the segment's runs, duplicates within a run (marks)
                // sit adjacent and merge after the sort.
                let mut clusters: Vec<(u64, Fx)> = Vec::new();
                for run in runs {
                    for g in &run.glyphs {
                        clusters.push((seg.char_start + u64::from(g.cluster), g.advance));
                    }
                }
                clusters.sort_unstable_by_key(|(c, _)| *c);
                for (c, adv) in clusters {
                    match atoms.last_mut() {
                        Some(last) if last.start == c => last.advance += adv,
                        _ => atoms.push(Atom {
                            start: c,
                            end: c + 1,
                            advance: adv,
                            is_space: false,
                        }),
                    }
                }
            }
            SegmentKind::Ruby(ruby) => {
                let base = runs_advance(&ruby.base);
                let annotation = runs_advance(&ruby.annotation);
                atoms.push(Atom {
                    start: seg.char_start,
                    end: seg.char_start + seg.char_len,
                    advance: base.max(annotation),
                    is_space: false,
                });
            }
        }
    }
    if atoms.is_empty() {
        return atoms;
    }
    // Coverage fix: glyphless chars fold into the preceding atom; a
    // glyphless head folds into the first.
    atoms.sort_by_key(|a| a.start);
    atoms[0].start = 0;
    for i in 0..atoms.len() - 1 {
        let next_start = atoms[i + 1].start;
        atoms[i].end = next_start;
    }
    if let Some(last) = atoms.last_mut() {
        last.end = n_chars;
    }
    for atom in &mut atoms {
        atom.is_space = chars.get(atom.start as usize) == Some(&' ')
            && chars[(atom.start + 1) as usize..atom.end as usize]
                .iter()
                .all(|c| is_glyphless_ignorable(*c));
    }
    atoms
}

/// Characters that projection retains but shaping emits without a glyph.
/// Keep this in lockstep with the shape module's ignorable set: here it
/// decides whether a space atom still trims at a line break after coverage
/// has folded following ignorables into it.
fn is_glyphless_ignorable(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\r'
            | '\u{00AD}'
            | '\u{061C}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
    )
}

fn runs_advance(runs: &[ShapedRun]) -> Fx {
    let mut sum = Fx::ZERO;
    for run in runs {
        for g in &run.glyphs {
            sum += g.advance;
        }
    }
    sum
}

/// UAX-14 break candidates mapped to atom boundaries: `(atom index,
/// mandatory)`, ascending. Opportunities inside an atom (ligature or
/// ruby interiors) are unusable and dropped. The end-of-text mandatory
/// candidate maps to `atoms.len()`.
fn candidates(text: &str, atoms: &[Atom], n_chars: u64) -> Vec<(usize, bool)> {
    // Byte offset of every char, plus the end.
    let mut byte_of_char: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    byte_of_char.push(text.len());
    let mut out = Vec::new();
    for (byte, opp) in linebreaks(text) {
        let Ok(char_idx) = byte_of_char.binary_search(&byte) else {
            continue; // never happens: breaks land on char boundaries
        };
        let c = char_idx as u64;
        if c == 0 {
            continue;
        }
        let mandatory = opp == BreakOpportunity::Mandatory;
        if c == n_chars {
            out.push((atoms.len(), mandatory));
        } else if let Ok(idx) = atoms.binary_search_by_key(&c, |a| a.start) {
            out.push((idx, mandatory));
        }
    }
    out
}
