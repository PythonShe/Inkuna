use std::path::Path;
use std::sync::Arc;

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
    assert_eq!(entries.len(), 25);
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
        assert!(entry.axes.is_empty());
    }
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
        let parsed = match rustybuzz::ttf_parser::Face::parse(&face.data, face.collection_index) {
            Ok(f) => f,
            Err(e) => panic!("face {id} must parse: {e}"),
        };
        parsed
            .names()
            .into_iter()
            .filter_map(|n| n.to_string())
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
    assert_eq!(file(24), "NotoSansSymbols2-Regular.ttf");
    expect(24, ["Noto", "Sans", "Symbols"]);
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(entry.id, i as u32);
    }
}

#[test]
fn cjk_region_mapping() {
    let reg = registry();
    let serif_reg = |lang: Option<&str>| reg.cjk(lang, true, FontWeight::Normal);
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
    assert_eq!(reg.cjk(Some("ja"), false, FontWeight::Normal), 20);
    assert_eq!(reg.cjk(Some("ja"), true, FontWeight::Bold), 13);
    assert_eq!(reg.cjk(None, false, FontWeight::Bold), 17);
}

#[test]
fn select_bold_italic() {
    let reg = registry();
    assert_eq!(
        reg.select(FontFamily::NotoSerif, FontStyle::Italic, FontWeight::Bold),
        3
    );
    assert_eq!(
        reg.select(FontFamily::NotoSerif, FontStyle::Normal, FontWeight::Normal),
        0
    );
    assert_eq!(
        reg.select(FontFamily::NotoSerif, FontStyle::Normal, FontWeight::Bold),
        2
    );
    assert_eq!(
        reg.select(FontFamily::NotoSans, FontStyle::Italic, FontWeight::Normal),
        5
    );
    assert_eq!(
        reg.select(FontFamily::NotoSans, FontStyle::Italic, FontWeight::Bold),
        7
    );
    assert_eq!(reg.symbols(), 24);
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
