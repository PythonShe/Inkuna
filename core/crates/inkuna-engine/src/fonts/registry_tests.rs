use std::path::Path;
use std::sync::Arc;

use read_fonts::types::NameId;
use read_fonts::{FontRef, TableProvider};

use super::FontRegistry;
use crate::settings::FontFamily;
use crate::style::{FontStyle, FontWeight};

/// The real shipped bytes: assets are product files, not fixtures, so
/// loading them honors the no-binary-fixtures rule.
fn repo_font_dir() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../assets/fonts"
    ))
}

fn registry() -> Arc<FontRegistry> {
    match FontRegistry::load(repo_font_dir()) {
        Ok(reg) => reg,
        Err(e) => panic!("repo font set must load: {e}"),
    }
}

#[test]
fn loads_repo_font_set() {
    let reg = registry();
    let entries = reg.entries();
    assert_eq!(entries.len(), super::FIRST_DYNAMIC_ID as usize);
    assert_eq!(super::FIRST_DYNAMIC_ID, 57);
    for entry in &entries {
        let face = reg.face(entry.id);
        assert!(face.upem > 0, "face {} upem", entry.id);
        assert!(!face.data.is_empty());
        assert_eq!(face.collection_index, entry.collection_index);
        assert!(
            Path::new(&entry.file_path).is_absolute(),
            "entry path must be absolute: {}",
            entry.file_path
        );
        assert_eq!(face.axes, entry.axes, "face/entry axes agree");
        if entry.id <= 28 {
            // Manifest faces shape at their default instances.
            assert!(entry.axes.is_empty(), "manifest id {} has axes", entry.id);
        } else {
            // Instance ids carry exactly one wght coordinate.
            assert_eq!(entry.axes.len(), 1, "instance id {}", entry.id);
            assert_eq!(entry.axes[0].tag, "wght");
        }
    }
}

/// The weight-instance block, exactly as the module doc's table fixes
/// it: four variable faces × seven wght coordinates, ids 29..=56, with
/// each instance backed by its base face's file and identity.
#[test]
fn instance_block_is_deterministic() {
    let reg = registry();
    let entries = reg.entries();
    let weights = [100.0, 200.0, 300.0, 500.0, 600.0, 800.0, 900.0];
    let bases = [0usize, 1, 4, 5]; // Serif R/I, Sans R/I
    for (b, &base) in bases.iter().enumerate() {
        for (s, &wght) in weights.iter().enumerate() {
            let id = 29 + b * weights.len() + s;
            let entry = &entries[id];
            assert_eq!(entry.id, id as u32);
            assert_eq!(entry.file_path, entries[base].file_path);
            assert_eq!(entry.collection_index, 0);
            assert_eq!(entry.post_script_name, entries[base].post_script_name);
            assert_eq!(entry.axes[0].value, wght, "id {id} wght");
            // Metrics reuse the base face's default instance.
            let face = reg.face(id as u32);
            let base_face = reg.face(base as u32);
            assert_eq!(face.upem, base_face.upem);
            assert_eq!(face.ascender, base_face.ascender);
            assert_eq!(face.descender, base_face.descender);
        }
    }
}

/// Every instance coordinate sits inside the face's actual fvar range —
/// the load-time clamp must be a no-op for the shipped Noto set.
#[test]
fn instance_weights_are_within_fvar_range() {
    let reg = registry();
    for entry in reg.entries() {
        if entry.axes.is_empty() {
            continue;
        }
        let face = reg.face(entry.id);
        let font = match FontRef::from_index(&face.data, face.collection_index) {
            Ok(f) => f,
            Err(e) => panic!("face {} must parse: {e}", entry.id),
        };
        let fvar = match font.fvar() {
            Ok(fvar) => fvar,
            Err(e) => panic!("face {} must be variable: {e}", entry.id),
        };
        let axes = match fvar.axes() {
            Ok(axes) => axes,
            Err(e) => panic!("face {} axes: {e}", entry.id),
        };
        let wght = axes
            .iter()
            .find(|a| a.axis_tag() == read_fonts::types::Tag::new(b"wght"))
            .unwrap_or_else(|| panic!("face {} has no wght axis", entry.id));
        let (min, max) = (wght.min_value().to_f64(), wght.max_value().to_f64());
        let v = entry.axes[0].value;
        assert!(
            (min..=max).contains(&v),
            "id {}: wght {v} outside fvar [{min}, {max}]",
            entry.id
        );
    }
}

fn parsed_post_script_name(reg: &FontRegistry, id: u32) -> String {
    let face = reg.face(id);
    let font = match FontRef::from_index(&face.data, face.collection_index) {
        Ok(font) => font,
        Err(e) => panic!("face {id} must parse: {e}"),
    };
    let names = match font.name() {
        Ok(names) => names,
        Err(e) => panic!("face {id} must have a name table: {e}"),
    };
    let data = names.string_data();
    match names
        .name_record()
        .into_iter()
        .find(|record| record.name_id() == NameId::POSTSCRIPT_NAME)
        .and_then(|record| record.string(data).ok())
    {
        Some(name) => name.to_string(),
        None => panic!("face {id} must have a readable PostScript name"),
    }
}

#[test]
fn entries_expose_post_script_identity_from_the_selected_face() {
    let reg = registry();
    let entries = reg.entries();

    let latin = &entries[0];
    assert_eq!(latin.post_script_name, "NotoSerif-Regular");
    assert_eq!(latin.post_script_name, parsed_post_script_name(&reg, latin.id));

    let cjk = &entries[10];
    assert_eq!(cjk.collection_index, 3, "Traditional Chinese in the OTC");
    assert_eq!(cjk.post_script_name, "NotoSerifCJKtc-Regular");
    assert_eq!(cjk.post_script_name, parsed_post_script_name(&reg, cjk.id));
}

#[test]
fn id_order_is_stable() {
    let reg = registry();
    let entries = reg.entries();
    let file = |id: usize| {
        Path::new(&entries[id].file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string()
    };
    // Reading faces 0..=7.
    assert_eq!(file(0), "NotoSerif.ttf");
    assert_eq!(file(1), "NotoSerif-Italic.ttf");
    assert_eq!(file(2), "NotoSerif-Bold.ttf");
    assert_eq!(file(3), "NotoSerif-BoldItalic.ttf");
    assert_eq!(file(4), "NotoSans.ttf");
    assert_eq!(file(5), "NotoSans-Italic.ttf");
    assert_eq!(file(6), "NotoSans-Bold.ttf");
    assert_eq!(file(7), "NotoSans-BoldItalic.ttf");
    // CJK: Serif before Sans; SC, TC, JP, KR; Regular before Bold.
    // Identity is proven by the name table of the face each id's
    // collection_index actually selects — NOT by echoing the code's
    // index constants — so a font bump that reorders the OTC fails
    // here instead of silently rendering SC text with JP glyphs.
    let names = |id: u32| -> Vec<String> {
        let face = reg.face(id);
        let font = match FontRef::from_index(&face.data, face.collection_index) {
            Ok(f) => f,
            Err(e) => panic!("face {id} must parse: {e}"),
        };
        let names = match font.name() {
            Ok(names) => names,
            Err(e) => panic!("face {id} must have a name table: {e}"),
        };
        let data = names.string_data();
        names
            .name_record()
            .into_iter()
            .filter_map(|record| record.string(data).ok().map(|name| name.to_string()))
            .collect()
    };
    let expect = |id: usize, tokens: [&str; 3]| {
        let found = names(id as u32);
        for token in tokens {
            assert!(
                found.iter().any(|n| n.contains(token)),
                "id {id}: no name table entry contains {token:?}: {found:?}"
            );
        }
    };
    // PostScript names in the noto-cjk OTCs carry the region as e.g.
    // "NotoSerifCJKsc-Regular".
    let regions = ["CJKsc", "CJKtc", "CJKjp", "CJKkr"];
    for (r, region) in regions.iter().enumerate() {
        let serif_regular = 8 + r * 2;
        expect(serif_regular, ["Serif", region, "Regular"]);
        expect(serif_regular + 1, ["Serif", region, "Bold"]);
        expect(serif_regular + 8, ["Sans", region, "Regular"]);
        expect(serif_regular + 9, ["Sans", region, "Bold"]);
    }
    // Hebrew 24..=27: Serif before Sans, Regular before Bold — between
    // the CJK block and Symbols. Identity proven by name table, like
    // the CJK faces.
    assert_eq!(file(24), "NotoSerifHebrew-Regular.ttf");
    expect(24, ["Serif", "Hebrew", "Regular"]);
    assert_eq!(file(25), "NotoSerifHebrew-Bold.ttf");
    expect(25, ["Serif", "Hebrew", "Bold"]);
    assert_eq!(file(26), "NotoSansHebrew-Regular.ttf");
    expect(26, ["Sans", "Hebrew", "Regular"]);
    assert_eq!(file(27), "NotoSansHebrew-Bold.ttf");
    expect(27, ["Sans", "Hebrew", "Bold"]);
    // Symbols stays LAST — the fallback chain's terminal face.
    assert_eq!(file(28), "NotoSansSymbols2-Regular.ttf");
    expect(28, ["Noto", "Sans", "Symbols"]);
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(entry.id, i as u32);
    }
}

#[test]
fn cjk_region_mapping() {
    let reg = registry();
    let serif_reg = |lang: Option<&str>| reg.cjk(lang, true, FontWeight::NORMAL);
    assert_eq!(serif_reg(Some("ja")), 12);
    assert_eq!(serif_reg(Some("ja-JP")), 12);
    assert_eq!(serif_reg(Some("ko")), 14);
    assert_eq!(serif_reg(Some("zh-Hant")), 10);
    assert_eq!(serif_reg(Some("zh-TW")), 10);
    assert_eq!(serif_reg(Some("zh-HK")), 10);
    assert_eq!(serif_reg(Some("zh")), 8);
    assert_eq!(serif_reg(Some("zh-Hans")), 8);
    assert_eq!(serif_reg(Some("en")), 8);
    assert_eq!(serif_reg(None), 8);
    // Sans block and bold offsets.
    assert_eq!(reg.cjk(Some("ja"), false, FontWeight::NORMAL), 20);
    assert_eq!(reg.cjk(Some("ja"), true, FontWeight::BOLD), 13);
    assert_eq!(reg.cjk(None, false, FontWeight::BOLD), 17);
}

#[test]
fn select_bold_italic() {
    let reg = registry();
    assert_eq!(
        reg.select(FontFamily::NotoSerif, FontStyle::Italic, FontWeight::BOLD),
        3
    );
    assert_eq!(
        reg.select(FontFamily::NotoSerif, FontStyle::Normal, FontWeight::NORMAL),
        0
    );
    assert_eq!(
        reg.select(FontFamily::NotoSerif, FontStyle::Normal, FontWeight::BOLD),
        2
    );
    assert_eq!(
        reg.select(FontFamily::NotoSans, FontStyle::Italic, FontWeight::NORMAL),
        5
    );
    assert_eq!(
        reg.select(FontFamily::NotoSans, FontStyle::Italic, FontWeight::BOLD),
        7
    );
    assert_eq!(reg.symbols(), 28);
}

/// Numeric selection: exact standard weights hit their instance ids,
/// 400/700 keep the base/static ids, and odd values follow the CSS
/// font-matching nearest rule (below-first under 400, 500-first in
/// 400..=500, above-first over 500).
#[test]
fn select_numeric_weights() {
    let reg = registry();
    let serif = |w: u16| reg.select(FontFamily::NotoSerif, FontStyle::Normal, FontWeight::new(w));
    // Exact standard weights: instance block ids for the serif upright
    // face are 29..=35 = [100, 200, 300, 500, 600, 800, 900].
    assert_eq!(serif(100), 29);
    assert_eq!(serif(300), 31);
    assert_eq!(serif(400), 0);
    assert_eq!(serif(500), 32);
    assert_eq!(serif(600), 33);
    assert_eq!(serif(700), 2);
    assert_eq!(serif(900), 35);
    // CSS nearest rule.
    assert_eq!(serif(350), 31, "below 400 goes down first (350 -> 300)");
    assert_eq!(serif(50), 29, "clamps up to 100 when nothing sits below");
    assert_eq!(serif(450), 32, "400..=500 checks toward 500 first");
    assert_eq!(serif(501), 33, "above 500 goes up first (501 -> 600)");
    assert_eq!(serif(650), 2, "650 -> 700 (static bold)");
    assert_eq!(serif(950), 35, "clamps down to 900 when nothing sits above");
    // Other faces land in their own blocks.
    assert_eq!(
        reg.select(FontFamily::NotoSerif, FontStyle::Italic, FontWeight::new(200)),
        37
    );
    assert_eq!(
        reg.select(FontFamily::NotoSans, FontStyle::Normal, FontWeight::new(900)),
        49
    );
    assert_eq!(
        reg.select(FontFamily::NotoSans, FontStyle::Italic, FontWeight::new(100)),
        50
    );
    // Static blocks threshold at 600.
    assert_eq!(reg.cjk(Some("ja"), true, FontWeight::new(599)), 12);
    assert_eq!(reg.cjk(Some("ja"), true, FontWeight::new(600)), 13);
    assert_eq!(reg.hebrew(true, FontWeight::new(599)), 24);
    assert_eq!(reg.hebrew(true, FontWeight::new(600)), 25);
}

#[test]
fn hebrew_family_weight() {
    let reg = registry();
    assert_eq!(reg.hebrew(true, FontWeight::NORMAL), 24);
    assert_eq!(reg.hebrew(true, FontWeight::BOLD), 25);
    assert_eq!(reg.hebrew(false, FontWeight::NORMAL), 26);
    assert_eq!(reg.hebrew(false, FontWeight::BOLD), 27);
    // Every Hebrew face actually maps a Hebrew letter (א) — the
    // fallback stage would be pointless tofu otherwise.
    for id in 24..=27 {
        let face = reg.face(id);
        let font = match FontRef::from_index(&face.data, face.collection_index) {
            Ok(f) => f,
            Err(e) => panic!("face {id} must parse: {e}"),
        };
        let gid = font
            .cmap()
            .ok()
            .and_then(|cmap| cmap.map_codepoint('\u{05D0}'));
        assert!(
            gid.is_some_and(|g| g.to_u32() != 0),
            "face {id} must cover Hebrew"
        );
    }
}

#[test]
fn missing_font_dir_fails() {
    let dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => panic!("tempdir: {e}"),
    };
    match FontRegistry::load(dir.path()) {
        Err(crate::EngineError::UnsupportedContent { detail }) => {
            assert!(
                detail.starts_with("font missing: NotoSerif.ttf"),
                "detail names the first missing file: {detail}"
            );
        }
        Ok(_) => panic!("empty dir must not load"),
        Err(e) => panic!("wrong error kind: {e}"),
    }
}
