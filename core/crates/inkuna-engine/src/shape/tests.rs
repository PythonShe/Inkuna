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
        families: &[],
        font_style: FontStyle::Normal,
        font_weight: FontWeight::NORMAL,
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

fn glyph_snapshot(runs: &[ShapedRun]) -> Vec<(u32, Vec<(u16, u32, Fx, Fx, Fx)>)> {
    runs.iter()
        .map(|run| {
            (
                run.font_id,
                run.glyphs
                    .iter()
                    .map(|glyph| {
                        (
                            glyph.glyph_id,
                            glyph.cluster,
                            glyph.advance,
                            glyph.offset_x,
                            glyph.offset_y,
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn shaping_output_stays_stable_for_latin_and_cjk() {
    let fonts = registry();
    let latin = glyph_snapshot(&shape_text("Inkuna", &ctx(fonts)));
    let cjk = glyph_snapshot(&shape_text("漢字", &ctx(fonts)));

    assert_eq!(
        latin,
        vec![(
            0,
            vec![
                (44, 0, Fx(376), Fx::ZERO, Fx::ZERO),
                (81, 1, Fx(660), Fx::ZERO, Fx::ZERO),
                (78, 2, Fx(599), Fx::ZERO, Fx::ZERO),
                (88, 3, Fx(650), Fx::ZERO, Fx::ZERO),
                (81, 4, Fx(660), Fx::ZERO, Fx::ZERO),
                (68, 5, Fx(577), Fx::ZERO, Fx::ZERO),
            ],
        )]
    );
    assert_eq!(
        cjk,
        vec![(
            8,
            vec![
                (58864, 0, Fx(1024), Fx::ZERO, Fx::ZERO),
                (15349, 1, Fx(1024), Fx::ZERO, Fx::ZERO),
            ],
        )]
    );
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
        font_weight: FontWeight::BOLD,
        ..ctx(fonts)
    };
    let runs = shape_text("Hello", &bold_ctx);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].font_id, 2, "Serif Bold");
    assert_eq!(runs[0].style.font_weight, FontWeight::BOLD);
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

#[test]
fn hebrew_falls_back_to_hebrew_face() {
    let fonts = registry();
    let runs = shape_text("שלום", &ctx(fonts));
    assert!(!runs.is_empty());
    for run in &runs {
        assert_eq!(run.font_id, 24, "Serif Hebrew Regular");
        assert!(
            run.glyphs.iter().all(|g| g.glyph_id != 0),
            "Hebrew must not shape to .notdef"
        );
    }
}

#[test]
fn hebrew_fallback_serif_sans_bold_italic() {
    let fonts = registry();
    let mut c = ctx(fonts);
    c.font_weight = FontWeight::BOLD;
    assert!(shape_text("א", &c).iter().all(|r| r.font_id == 25));
    c.family = FontFamily::NotoSans;
    assert!(shape_text("א", &c).iter().all(|r| r.font_id == 27));
    // No Hebrew italics exist: italic requests map to regular.
    c.family = FontFamily::NotoSerif;
    c.font_weight = FontWeight::NORMAL;
    c.font_style = FontStyle::Italic;
    assert!(shape_text("א", &c).iter().all(|r| r.font_id == 24));
}

#[test]
fn non_hebrew_misses_skip_hebrew_stage() {
    // A CJK char missing from the reading face must still fall to the
    // CJK stage, never the Hebrew face.
    let fonts = registry();
    let runs = shape_text("汉", &ctx(fonts));
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].font_id, 8, "Serif CJK SC Regular");
}

/// D2 word-spacing audit: `(cluster, advance)` of every U+0020 glyph.
fn space_advances(runs: &[ShapedRun], text: &str) -> Vec<(u32, Fx)> {
    let spaces: Vec<u32> = text
        .chars()
        .enumerate()
        .filter(|(_, c)| *c == ' ')
        .map(|(i, _)| i as u32)
        .collect();
    runs.iter()
        .flat_map(|r| r.glyphs.iter())
        .filter(|g| spaces.contains(&g.cluster))
        .map(|g| (g.cluster, g.advance))
        .collect()
}

/// Every inter-word gap must be one space glyph in the SAME face as the
/// letters, with a sane advance: nonzero, below half an em, and equal
/// across the sentence. Guards suspects 1 and 2 of the word-spacing
/// report: instance-path shaping (wght via ShaperInstance) and
/// registered system / publisher reading faces.
fn assert_sane_spaces(runs: &[ShapedRun], text: &str, label: &str) -> Fx {
    assert_eq!(runs.len(), 1, "{label}: sentence must stay one run, got {runs:?}");
    let spaces = space_advances(runs, text);
    assert_eq!(spaces.len(), 2, "{label}: both spaces must emit a glyph");
    let em = runs[0].size;
    let (lo, hi) = (Fx(em.0 / 8), Fx(em.0 / 2));
    for (cluster, adv) in &spaces {
        assert!(
            *adv > lo && *adv < hi,
            "{label}: space at {cluster} has advance {adv:?}, outside ({lo:?}, {hi:?})"
        );
    }
    assert_eq!(spaces[0].1, spaces[1].1, "{label}: unequal space advances");
    spaces[0].1
}

#[test]
fn word_gaps_sane_across_weights_on_the_instance_path() {
    let fonts = registry();
    let text = "one two three";
    let mut advances = Vec::new();
    for weight in [400u16, 600, 700] {
        let c = ShapeContext {
            font_weight: FontWeight::new(weight),
            ..ctx(fonts)
        };
        let runs = shape_text(text, &c);
        advances.push(assert_sane_spaces(&runs, text, &format!("NotoSerif {weight}")));
    }
    // 400 (default instance), 600 (bold-toggle floor, ShaperInstance
    // path) and 700 (static Bold face) must agree within 25%.
    for pair in advances.windows(2) {
        let (a, b) = (pair[0].0 as f64, pair[1].0 as f64);
        assert!(
            (a - b).abs() / a < 0.25,
            "space advance jumped across weights: {advances:?}"
        );
    }
}

#[test]
fn word_gaps_sane_with_registered_system_face() {
    // A registered platform face replacing the Reading stage — the
    // bundled Sans file stands in for Roboto/New York.
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/fonts"));
    let face = crate::fonts::SystemFontFace {
        role: crate::fonts::SystemFontRole::Serif,
        italic: false,
        weight: 400,
        file_path: dir.join("NotoSans.ttf").to_string_lossy().into_owned(),
        post_script_name: None,
        ttc_hint: None,
    };
    let (fonts, warnings) =
        FontRegistry::load_with_system(dir, std::slice::from_ref(&face)).expect("registry loads");
    assert!(warnings.is_empty(), "system stand-in must register: {warnings:?}");
    let text = "one two three";
    for weight in [400u16, 600] {
        let c = ShapeContext {
            family: FontFamily::SystemSerif,
            font_weight: FontWeight::new(weight),
            ..ctx(&fonts)
        };
        let runs = shape_text(text, &c);
        assert!(
            runs[0].font_id >= crate::fonts::FIRST_DYNAMIC_ID,
            "system-serif must shape with the registered face, got id {}",
            runs[0].font_id
        );
        assert_sane_spaces(&runs, text, &format!("system face {weight}"));
    }
}

#[test]
fn word_gaps_sane_with_publisher_face() {
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/fonts"));
    let base = FontRegistry::load(dir).expect("registry loads");
    let spec = crate::fonts::PublisherFaceSpec {
        file_path: dir.join("NotoSans.ttf").into(),
        family: "PubBody".to_string(),
        italic: false,
        weight: (100, 900),
        unicode_ranges: None,
    };
    let fonts = FontRegistry::with_publisher(&base, std::slice::from_ref(&spec));
    let families = [crate::style::FamilyName::Named("PubBody".to_string())];
    let text = "one two three";
    for weight in [400u16, 600] {
        let c = ShapeContext {
            family: FontFamily::Publisher,
            families: &families,
            font_weight: FontWeight::new(weight),
            ..ctx(&fonts)
        };
        let runs = shape_text(text, &c);
        assert!(
            runs[0].font_id >= base.next_free_id(),
            "publisher stack must shape with the publisher face, got id {}",
            runs[0].font_id
        );
        assert_sane_spaces(&runs, text, &format!("publisher face {weight}"));
    }
}

/// C1: a publisher face that won the stack's first entry but lacks
/// coverage must hand missing clusters to the NEXT stack entry, not
/// straight to the bundled CJK fallback. The first face is the Hebrew
/// Noto (no Greek); the second covers Greek.
#[test]
fn uncovered_cluster_walks_to_next_stack_entry() {
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/fonts"));
    let base = FontRegistry::load(dir).expect("registry loads");
    let specs = [
        crate::fonts::PublisherFaceSpec {
            file_path: dir.join("NotoSerifHebrew-Regular.ttf"),
            family: "HebOnly".to_string(),
            italic: false,
            weight: (400, 400),
            unicode_ranges: None,
        },
        crate::fonts::PublisherFaceSpec {
            file_path: dir.join("NotoSans.ttf"),
            family: "FullBody".to_string(),
            italic: false,
            weight: (100, 900),
            unicode_ranges: None,
        },
    ];
    let fonts = FontRegistry::with_publisher(&base, &specs);
    // Static Hebrew face: one id at the block base; NotoSans variable:
    // nine instance ids right after it.
    let first_id = base.next_free_id();
    let second_ids = (first_id + 1)..(first_id + 10);
    let families = [
        crate::style::FamilyName::Named("HebOnly".to_string()),
        crate::style::FamilyName::Named("FullBody".to_string()),
    ];
    let c = ShapeContext {
        family: FontFamily::Publisher,
        families: &families,
        ..ctx(&fonts)
    };
    let runs = shape_text("Ω", &c);
    assert_eq!(runs.len(), 1);
    assert!(
        second_ids.contains(&runs[0].font_id),
        "Greek must shape with the stack's second face, got id {}",
        runs[0].font_id
    );
    // Sanity: text the first face covers stays on the first face.
    let runs = shape_text("שלום", &c);
    assert!(runs.iter().all(|r| r.font_id == first_id));
}

/// C3: a subsetted face's declared `unicode-range` gates what it may
/// claim — codepoints outside the ranges walk to the next stack entry
/// even though the face carries their glyphs.
#[test]
fn unicode_range_gates_publisher_face_claims() {
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/fonts"));
    let base = FontRegistry::load(dir).expect("registry loads");
    let specs = [
        crate::fonts::PublisherFaceSpec {
            file_path: dir.join("NotoSerif.ttf"),
            family: "CapsOnly".to_string(),
            italic: false,
            weight: (100, 900),
            // A–Z only, though the file covers far more.
            unicode_ranges: Some(vec![(0x41, 0x5A)]),
        },
        crate::fonts::PublisherFaceSpec {
            file_path: dir.join("NotoSans.ttf"),
            family: "FullBody".to_string(),
            italic: false,
            weight: (100, 900),
            unicode_ranges: None,
        },
    ];
    let fonts = FontRegistry::with_publisher(&base, &specs);
    let first_ids = base.next_free_id()..(base.next_free_id() + 9);
    let second_ids = first_ids.end..(first_ids.end + 9);
    let families = [
        crate::style::FamilyName::Named("CapsOnly".to_string()),
        crate::style::FamilyName::Named("FullBody".to_string()),
    ];
    let c = ShapeContext {
        family: FontFamily::Publisher,
        families: &families,
        ..ctx(&fonts)
    };
    let runs = shape_text("AB ab", &c);
    let of = |cluster: u32| {
        runs.iter()
            .find(|r| r.glyphs.iter().any(|g| g.cluster == cluster))
            .map(|r| r.font_id)
            .expect("cluster shaped")
    };
    for caps in [0u32, 1] {
        assert!(
            first_ids.contains(&of(caps)),
            "A–Z inside the declared range must stay on the subsetted face"
        );
    }
    for outside in [2u32, 3, 4] {
        assert!(
            second_ids.contains(&of(outside)),
            "codepoints outside the declared range must walk to the next \
             stack entry, got id {}",
            of(outside)
        );
    }
}

/// C2: under the publisher reading font, the CJK and Hebrew fallback
/// serif-ness follows the stack's generic keyword — `…, sans-serif`
/// gets the sans Notos, `…, serif` (and no generic) the serif ones.
#[test]
fn stack_generic_drives_cjk_and_hebrew_fallback_flavor() {
    let fonts = registry();
    let sans_stack = [
        crate::style::FamilyName::Named("NoSuchFamily".to_string()),
        crate::style::FamilyName::SansSerif,
    ];
    let serif_stack = [
        crate::style::FamilyName::Named("NoSuchFamily".to_string()),
        crate::style::FamilyName::Serif,
    ];
    let no_generic = [crate::style::FamilyName::Named("NoSuchFamily".to_string())];
    let shape_one = |families: &[crate::style::FamilyName], text: &str| {
        let c = ShapeContext {
            family: FontFamily::Publisher,
            families,
            ..ctx(fonts)
        };
        let runs = shape_text(text, &c);
        assert_eq!(runs.len(), 1);
        runs[0].font_id
    };
    // Sans CJK SC Regular is id 16, serif 8; sans Hebrew Regular 26, serif 24.
    assert_eq!(shape_one(&sans_stack, "中"), 16);
    assert_eq!(shape_one(&serif_stack, "中"), 8);
    assert_eq!(shape_one(&no_generic, "中"), 8);
    assert_eq!(shape_one(&sans_stack, "ש"), 26);
    assert_eq!(shape_one(&serif_stack, "ש"), 24);
}
