//! Justification: distributes a line's deficit over its stretch gaps in
//! exact 1/64 units. Inter-word (space) stretch first; CJK
//! inter-character gaps join when the line has no spaces, or when the
//! spaces alone would each grow beyond half an em. The remainder is
//! handed out one unit at a time to the first gaps in visual order —
//! never divided as a float.

use unicode_script::{Script, UnicodeScript};

use crate::fixed::Fx;

use super::assemble::Item;

#[derive(Clone, Copy, PartialEq, Eq)]
enum GapKind {
    Space,
    Cjk,
}

/// One stretchable gap: the glyph whose advance grows.
struct Gap {
    item: usize,
    run: usize,
    glyph: usize,
    kind: GapKind,
    /// The run's font size, for the half-em space cap.
    size: Fx,
}

/// Stretches the line's glyph advances by `deficit`. Ruby items are
/// atomic — never stretched inside, and gaps never form across their
/// edges. Returns false (line stays ragged) when no gap exists.
pub(super) fn stretch(items: &mut [Item], deficit: Fx, chars: &[char], para_char_start: u64) -> bool {
    let gaps = collect_gaps(items, chars, para_char_start);
    let spaces = gaps.iter().filter(|g| g.kind == GapKind::Space).count();
    let cjk = gaps.len() - spaces;

    let chosen: Vec<&Gap> = if spaces > 0 {
        let per_space = deficit.0 / spaces as i32;
        let half_em = gaps
            .iter()
            .filter(|g| g.kind == GapKind::Space)
            .map(|g| g.size.0 / 2)
            .min()
            .unwrap_or(i32::MAX);
        if per_space > half_em && cjk > 0 {
            gaps.iter().collect()
        } else {
            gaps.iter().filter(|g| g.kind == GapKind::Space).collect()
        }
    } else if cjk > 0 {
        gaps.iter().filter(|g| g.kind == GapKind::Cjk).collect()
    } else {
        return false;
    };
    if chosen.is_empty() {
        return false;
    }

    // Exact integer split: `base` to every gap, one extra unit to the
    // first `rem` gaps in visual order. Deterministic by construction.
    let n = chosen.len() as i32;
    let base = deficit.0.div_euclid(n);
    let rem = deficit.0.rem_euclid(n);
    for (i, gap) in chosen.iter().enumerate() {
        let extra = if (i as i32) < rem { 1 } else { 0 };
        let g = &mut items[gap.item].base[gap.run].run.glyphs[gap.glyph];
        g.advance += Fx(base + extra);
    }
    true
}

/// All stretch gaps of the line, in visual order: non-trailing space
/// glyphs, and boundaries between two adjacent CJK glyphs (the gap
/// grows the leading glyph's advance).
fn collect_gaps(items: &[Item], chars: &[char], para_char_start: u64) -> Vec<Gap> {
    let char_at = |cluster: u32| -> Option<char> {
        let local = u64::from(cluster).checked_sub(para_char_start)?;
        chars.get(usize::try_from(local).ok()?).copied()
    };
    let mut gaps = Vec::new();
    // The previous non-ruby glyph's position and CJK-ness, tracked
    // across run and item boundaries; ruby items reset it.
    let mut prev: Option<(usize, usize, usize, bool, Fx)> = None;
    for (i, item) in items.iter().enumerate() {
        if item.is_ruby {
            prev = None;
            continue;
        }
        for (r, wr) in item.base.iter().enumerate() {
            for (k, g) in wr.run.glyphs.iter().enumerate() {
                let ch = char_at(g.cluster);
                if ch == Some(' ') && g.advance > Fx::ZERO {
                    gaps.push(Gap {
                        item: i,
                        run: r,
                        glyph: k,
                        kind: GapKind::Space,
                        size: wr.run.size,
                    });
                }
                let is_cjk = ch.is_some_and(cjk_char);
                if let Some((pi, pr, pk, prev_cjk, prev_size)) = prev {
                    if prev_cjk && is_cjk {
                        gaps.push(Gap {
                            item: pi,
                            run: pr,
                            glyph: pk,
                            kind: GapKind::Cjk,
                            size: prev_size,
                        });
                    }
                }
                prev = Some((i, r, k, is_cjk, wr.run.size));
            }
        }
    }
    gaps
}

/// The scripts that stretch inter-character in justified CJK text.
fn cjk_char(c: char) -> bool {
    matches!(
        c.script(),
        Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul | Script::Bopomofo
    )
}
