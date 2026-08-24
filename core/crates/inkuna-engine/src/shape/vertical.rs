//! The single harfrust pass, owning direction, features, and run
//! orientation. In vertical mode, CJK-class items shape top-to-bottom
//! with `vert`+`vrt2` (the font applies vertical presentation forms)
//! and stay upright; horizontal-script items shape horizontally with
//! the same face and are marked `SidewaysRotated` — their advance is
//! consumed along the vertical line axis and the shells rotate per run.

use unicode_script::Script;

use super::itemize::Item;
use super::shape::{RawGlyph, RunOrientation, ShapeContext};

/// One harfrust pass over one slice with one face. Clusters are
/// explicit global char offsets.
pub(super) fn shape_once(
    slice: &str,
    start_char: usize,
    font_id: u32,
    item: &Item,
    ctx: &ShapeContext,
) -> (Vec<RawGlyph>, RunOrientation) {
    let upright = ctx.vertical && item_upright(item.script, slice);
    let orientation = if !ctx.vertical || upright {
        RunOrientation::Upright
    } else {
        RunOrientation::SidewaysRotated
    };

    let loaded = ctx.fonts.face(font_id);
    let Ok(font) = harfrust::FontRef::from_index(&loaded.data, loaded.collection_index) else {
        // Impossible past registry load; never drop text silently at
        // shape time either.
        return (Vec::new(), orientation);
    };
    let mut buf = harfrust::UnicodeBuffer::new();
    for (k, ch) in slice.chars().enumerate() {
        buf.add(ch, (start_char + k) as u32);
    }
    let vertical_features = [
        harfrust::Feature::new(harfrust::Tag::new(b"vert"), 1, ..),
        harfrust::Feature::new(harfrust::Tag::new(b"vrt2"), 1, ..),
    ];
    let features: &[harfrust::Feature] = if upright {
        buf.set_direction(harfrust::Direction::TopToBottom);
        &vertical_features
    } else {
        // Sideways-rotated runs shape HORIZONTALLY with the same face.
        buf.set_direction(if item.bidi_level % 2 == 1 {
            harfrust::Direction::RightToLeft
        } else {
            harfrust::Direction::LeftToRight
        });
        &[]
    };
    if let Some(script) = rb_script(item.script) {
        buf.set_script(script);
    }
    if let Some(lang) = ctx.lang.and_then(|l| l.parse::<harfrust::Language>().ok()) {
        buf.set_language(lang);
    }
    // Weight-instance faces carry `wght` coordinates; shaping must
    // apply them or advances would come from the default instance.
    let variations: Vec<harfrust::Variation> = loaded
        .axes
        .iter()
        .filter_map(|axis| {
            let tag: [u8; 4] = axis.tag.as_bytes().try_into().ok()?;
            Some(harfrust::Variation {
                tag: harfrust::Tag::new(&tag),
                value: axis.value as f32,
            })
        })
        .collect();
    let instance = (!variations.is_empty())
        .then(|| harfrust::ShaperInstance::from_variations(&font, &variations));
    let shaper_data = harfrust::ShaperData::new(&font);
    let out = shaper_data
        .shaper(&font)
        .instance(instance.as_ref())
        .build()
        .shape(buf, harfrust::ShapeOptions::new().features(features));
    let raw = out
        .glyph_infos()
        .iter()
        .zip(out.glyph_positions())
        .map(|(info, pos)| RawGlyph {
            glyph_id: info.glyph_id,
            cluster: info.cluster,
            x_advance: pos.x_advance,
            y_advance: pos.y_advance,
            x_offset: pos.x_offset,
            y_offset: pos.y_offset,
        })
        .collect();
    (raw, orientation)
}

/// Whether an item stays upright in vertical writing: the CJK scripts,
/// plus Common/Inherited items made purely of CJK symbols/punctuation,
/// vertical/compat forms, or full-width forms. (Common punctuation
/// adjacent to a real script merged into that script's item at
/// itemization, so this only decides isolated punctuation items.)
fn item_upright(script: Script, slice: &str) -> bool {
    match script {
        Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul | Script::Bopomofo => {
            true
        }
        Script::Common | Script::Inherited => slice.chars().all(upright_char),
        _ => false,
    }
}

/// CJK Symbols and Punctuation, Vertical Forms, CJK Compatibility
/// Forms, Halfwidth and Fullwidth Forms.
fn upright_char(c: char) -> bool {
    matches!(
        c as u32,
        0x3000..=0x303F | 0xFE10..=0xFE1F | 0xFE30..=0xFE4F | 0xFF00..=0xFFEF
    )
}

/// The harfrust script for a unicode-script value, via ISO 15924.
fn rb_script(script: Script) -> Option<harfrust::Script> {
    let bytes: [u8; 4] = script.short_name().as_bytes().try_into().ok()?;
    harfrust::Script::from_iso15924_tag(harfrust::Tag::new(&bytes))
}
