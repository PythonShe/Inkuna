use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::fixed::Fx;
use crate::fonts::FontRegistry;
use crate::settings::FontFamily;
use crate::style::{FontStyle, FontWeight};

use super::{shape_text, RunOrientation, ShapeContext, ShapedRun};

/// The real shipped bytes (product files, not fixtures), loaded once
/// for the whole test binary.
fn registry() -> &'static FontRegistry {
    static REG: OnceLock<Arc<FontRegistry>> = OnceLock::new();
    REG.get_or_init(|| {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/fonts"));
        FontRegistry::load(dir).expect("repo font set must load")
    })
}

fn ctx(fonts: &FontRegistry) -> ShapeContext<'_> {
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
        base_rtl: false,
    }
}

fn all_clusters(runs: &[ShapedRun]) -> Vec<u32> {
    let mut clusters: Vec<u32> = runs
        .iter()
        .flat_map(|r| r.glyphs.iter().map(|g| g.cluster))
        .collect();
    clusters.sort_unstable();
    clusters.dedup();
    clusters
}

#[test]
fn latin_shapes_with_reading_face() {
    let fonts = registry();
    let runs = shape_text("Hello", &ctx(fonts));
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.font_id, 0, "Serif Regular");
    assert_eq!(run.glyphs.len(), 5);
    assert_eq!(run.orientation, RunOrientation::Upright);
    for g in &run.glyphs {
        assert_ne!(g.glyph_id, 0);
        assert!(g.advance > Fx::ZERO);
    }
}

#[test]
fn cjk_falls_back_to_cjk_face() {
    let fonts = registry();
    let runs = shape_text("汉字", &ctx(fonts));
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.font_id, 8, "Serif CJK SC Regular (no lang -> SC)");
    assert_eq!(run.glyphs.len(), 2);
    for g in &run.glyphs {
        assert_ne!(g.glyph_id, 0);
        assert!(g.advance > Fx::ZERO);
    }
}

#[test]
fn mixed_script_splits_runs() {
    let fonts = registry();
    let runs = shape_text("abc汉def", &ctx(fonts));
    assert!(runs.len() >= 3, "got {} runs", runs.len());
    // Cluster offsets partition the 7 chars.
    assert_eq!(all_clusters(&runs), vec![0, 1, 2, 3, 4, 5, 6]);
    // Latin on the reading face, Han on a CJK face.
    assert_eq!(runs[0].font_id, 0);
    assert!(runs.iter().any(|r| r.font_id == 8));
    // Runs are in logical order.
    let firsts: Vec<u32> = runs
        .iter()
        .map(|r| r.glyphs.iter().map(|g| g.cluster).min().unwrap())
        .collect();
    let mut sorted = firsts.clone();
    sorted.sort_unstable();
    assert_eq!(firsts, sorted);
}

#[test]
fn bidi_levels_split() {
    let fonts = registry();
    let runs = shape_text("שלום abc", &ctx(fonts));
    assert!(runs.len() >= 2);
    let hebrew = runs
        .iter()
        .find(|r| r.glyphs.iter().any(|g| g.cluster == 0))
        .expect("hebrew run");
    let latin = runs
        .iter()
        .find(|r| r.glyphs.iter().any(|g| g.cluster == 5))
        .expect("latin run");
    assert_eq!(hebrew.bidi_level % 2, 1, "RTL run level is odd");
    assert_eq!(latin.bidi_level % 2, 0);
    assert_ne!(hebrew.bidi_level, latin.bidi_level);
    // RTL glyph output is visual order: descending clusters.
    let hebrew_clusters: Vec<u32> = hebrew.glyphs.iter().map(|g| g.cluster).collect();
    let mut descending = hebrew_clusters.clone();
    descending.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(hebrew_clusters, descending);
}

#[test]
fn unknown_char_reaches_notdef() {
    let fonts = registry();
    let runs = shape_text("\u{10FFFD}", &ctx(fonts));
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.font_id, 0, "terminal stage is the reading face");
    assert_eq!(run.glyphs.len(), 1);
    assert_eq!(run.glyphs[0].glyph_id, 0, ".notdef retained, never dropped");
    assert!(run.glyphs[0].advance >= Fx::ZERO);
}

#[test]
fn letter_spacing_adds_per_cluster() {
    let fonts = registry();
    let plain = shape_text("abc", &ctx(fonts));
    let spacing = Fx(64);
    let spaced_ctx = ShapeContext {
        letter_spacing: spacing,
        ..ctx(fonts)
    };
    let spaced = shape_text("abc", &spaced_ctx);
    assert_eq!(plain.len(), 1);
    assert_eq!(spaced.len(), 1);
    for (p, s) in plain[0].glyphs.iter().zip(&spaced[0].glyphs) {
        assert_eq!(s.advance, p.advance + spacing, "delta equals the spacing");
    }
}

#[test]
fn word_spacing_adds_to_spaces_only() {
    let fonts = registry();
    let plain = shape_text("a b", &ctx(fonts));
    let spacing = Fx(128);
    let spaced_ctx = ShapeContext {
        word_spacing: spacing,
        ..ctx(fonts)
    };
    let spaced = shape_text("a b", &spaced_ctx);
    let delta: Vec<Fx> = plain[0]
        .glyphs
        .iter()
        .zip(&spaced[0].glyphs)
        .map(|(p, s)| s.advance - p.advance)
        .collect();
    assert_eq!(delta, vec![Fx::ZERO, spacing, Fx::ZERO]);
}

#[test]
fn bold_selects_bold_face() {
    let fonts = registry();
    let bold_ctx = ShapeContext {
        font_weight: FontWeight::Bold,
        ..ctx(fonts)
    };
    let runs = shape_text("Hello", &bold_ctx);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].font_id, 2, "Serif Bold");
    assert_eq!(runs[0].style.font_weight, FontWeight::Bold);
}

#[test]
fn empty_input_yields_no_runs() {
    let fonts = registry();
    assert!(shape_text("", &ctx(fonts)).is_empty());
}
