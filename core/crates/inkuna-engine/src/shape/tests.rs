use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::fixed::Fx;
use crate::fonts::FontRegistry;
use crate::settings::FontFamily;
use crate::style::{FontStyle, FontWeight, RubyPosition};

use super::{shape_ruby, shape_text, RunOrientation, ShapeContext, ShapedRun};

/// The real shipped bytes (product files, not fixtures), loaded once
/// for the whole test binary.
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

#[test]
fn vertical_cjk_uses_vert_feature() {
    let fonts = registry();
    let horizontal = shape_text("「漢」", &ctx(fonts));
    let vertical_ctx = ShapeContext {
        vertical: true,
        ..ctx(fonts)
    };
    let vertical = shape_text("「漢」", &vertical_ctx);
    assert_eq!(vertical.len(), 1);
    let run = &vertical[0];
    assert_eq!(run.orientation, RunOrientation::Upright);
    for g in &run.glyphs {
        assert_ne!(g.glyph_id, 0);
        assert!(g.advance > Fx::ZERO, "vertical advance magnitude");
    }
    // The vertical presentation form of 「 (cluster 0) is a different
    // glyph than its horizontal shape.
    let glyph_at = |runs: &[ShapedRun], cluster: u32| {
        runs.iter()
            .flat_map(|r| r.glyphs.iter())
            .find(|g| g.cluster == cluster)
            .map(|g| g.glyph_id)
            .expect("cluster present")
    };
    assert_ne!(glyph_at(&vertical, 0), glyph_at(&horizontal, 0));
    assert_ne!(glyph_at(&vertical, 2), glyph_at(&horizontal, 2));
    // The ideograph itself keeps its glyph.
    assert_eq!(glyph_at(&vertical, 1), glyph_at(&horizontal, 1));
}

#[test]
fn latin_in_vertical_is_sideways() {
    let fonts = registry();
    let vertical_ctx = ShapeContext {
        vertical: true,
        ..ctx(fonts)
    };
    let runs = shape_text("abc漢", &vertical_ctx);
    assert!(runs.len() >= 2);
    let latin = runs
        .iter()
        .find(|r| r.glyphs.iter().any(|g| g.cluster == 0))
        .expect("latin run");
    let cjk = runs
        .iter()
        .find(|r| r.glyphs.iter().any(|g| g.cluster == 3))
        .expect("cjk run");
    assert_eq!(latin.orientation, RunOrientation::SidewaysRotated);
    assert_eq!(cjk.orientation, RunOrientation::Upright);
    for g in latin.glyphs.iter().chain(&cjk.glyphs) {
        assert!(g.advance > Fx::ZERO);
    }
}

#[test]
fn ruby_annotation_scaled_and_mapped() {
    let fonts = registry();
    let base_ctx = ctx(fonts);
    let ruby = shape_ruby("漢字", "かんじ", &base_ctx, (1, 2), RubyPosition::Over);
    assert!(!ruby.base.is_empty());
    assert!(!ruby.annotation.is_empty());
    assert_eq!(ruby.position, RubyPosition::Over);
    let expected_size = base_ctx.size.mul_ratio(1, 2);
    for run in &ruby.annotation {
        assert_eq!(run.size, expected_size, "base size × ruby scale");
        assert!(run.style.is_ruby);
        for g in &run.glyphs {
            assert_eq!(g.cluster, 0, "annotation clusters point at the base start");
        }
    }
    for run in &ruby.base {
        assert!(!run.style.is_ruby);
    }
}

#[test]
fn ruby_empty_annotation_is_plain_base() {
    let fonts = registry();
    let ruby = shape_ruby("漢字", "", &ctx(fonts), (1, 2), RubyPosition::Under);
    assert!(!ruby.base.is_empty());
    assert!(ruby.annotation.is_empty());
    assert_eq!(
        ruby.position,
        RubyPosition::Under,
        "position carried through"
    );
}

#[test]
fn ruby_position_carried_through() {
    let fonts = registry();
    let ruby = shape_ruby("漢字", "かんじ", &ctx(fonts), (1, 2), RubyPosition::Under);
    assert_eq!(ruby.position, RubyPosition::Under);
}

#[test]
fn newline_emits_no_glyph_and_no_fallback_run() {
    let fonts = registry();
    let runs = shape_text("ab\ncd", &ctx(fonts));
    // The \n never walks the fallback chain: one reading-face run, not
    // a split with a symbols/.notdef stage run in the middle.
    assert_eq!(runs.len(), 1, "no fallback-stage runs for \\n");
    let run = &runs[0];
    assert_eq!(run.font_id, 0, "reading face");
    // No glyph for the \n cluster (offset 2): zero advance contributed.
    let clusters: Vec<u32> = run.glyphs.iter().map(|g| g.cluster).collect();
    assert_eq!(clusters, vec![0, 1, 3, 4]);
    assert!(run.glyphs.iter().all(|g| g.glyph_id != 0));
}

#[test]
fn ignorable_only_input_yields_no_runs() {
    let fonts = registry();
    assert!(shape_text("\n", &ctx(fonts)).is_empty());
    assert!(shape_text("\t\r\u{00AD}\u{200B}\u{FEFF}", &ctx(fonts)).is_empty());
}

#[test]
fn letter_spacing_applies_once_per_cluster_across_run_splits() {
    let fonts = registry();
    let plain = shape_text("a汉b", &ctx(fonts));
    let spacing = Fx(64);
    let spaced_ctx = ShapeContext {
        letter_spacing: spacing,
        ..ctx(fonts)
    };
    let spaced = shape_text("a汉b", &spaced_ctx);
    let total = |runs: &[ShapedRun]| {
        runs.iter()
            .flat_map(|r| r.glyphs.iter())
            .fold(Fx::ZERO, |acc, g| acc + g.advance)
    };
    // Three clusters -> exactly three spacings (CSS semantics: the
    // last cluster of each run carries trailing spacing, and fallback
    // splits never double-space a boundary).
    assert_eq!(total(&spaced), total(&plain) + Fx(3 * 64));
}

#[test]
fn fallback_split_output_locked() {
    // Locks shaped output across the O(n^2)->O(n) cluster-grouping
    // rewrite: mixed-script text with multiple fallback splits keeps
    // full cluster coverage, logical run order, and per-run font
    // consistency.
    let fonts = registry();
    let text = "abc汉字def漢ghi";
    let runs = shape_text(text, &ctx(fonts));
    assert_eq!(all_clusters(&runs), (0..12).collect::<Vec<u32>>());
    let firsts: Vec<u32> = runs
        .iter()
        .map(|r| r.glyphs.iter().map(|g| g.cluster).min().unwrap())
        .collect();
    let mut sorted = firsts.clone();
    sorted.sort_unstable();
    assert_eq!(firsts, sorted, "runs stay in logical order");
    for run in &runs {
        assert!(matches!(run.font_id, 0 | 8), "reading or CJK face only");
        assert!(run.glyphs.iter().all(|g| g.glyph_id != 0));
    }
}
