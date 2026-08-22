use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::fixed::Fx;
use crate::fonts::FontRegistry;
use crate::settings::FontFamily;
use crate::shape::{shape_text, ShapeContext};
use crate::style::{FontStyle, FontWeight, TextAlign};

use super::{
    break_paragraph, LineOptions, SegmentKind, ShapedParagraph, ShapedSegment,
    MAX_LINES_PER_PARAGRAPH,
};

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

fn context(fonts: &FontRegistry) -> ShapeContext<'_> {
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

#[test]
fn trailing_space_followed_by_zero_width_space_is_excluded_from_measure() {
    let fonts = registry();
    let text = "a \u{200B}b";
    let runs = shape_text(text, &context(fonts));
    let char_len = text.chars().count() as u64;
    let p = ShapedParagraph {
        text: text.to_string(),
        char_range: 0..char_len,
        segments: vec![ShapedSegment {
            char_start: 0,
            char_len,
            kind: SegmentKind::Text(runs),
        }],
        base_rtl: false,
        fonts,
    };
    let a_advance = shape_text("a", &context(fonts))[0].glyphs[0].advance;
    let space_advance = shape_text(" ", &context(fonts))[0].glyphs[0].advance;
    let lines = break_paragraph(
        &p,
        a_advance + space_advance,
        &LineOptions {
            justify: TextAlign::Start,
            first_line_indent: Fx::ZERO,
            max_lines: MAX_LINES_PER_PARAGRAPH,
        },
    );

    assert!(lines.len() >= 2, "zero-width space supplies the break");
    assert_eq!(
        lines[0].inline_extent, a_advance,
        "trailing space is not measured"
    );
}
