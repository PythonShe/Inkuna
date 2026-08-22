use std::path::Path;
use std::sync::Arc;

use super::FontRegistry;
use crate::settings::FontFamily;
use crate::style::{FontStyle, FontWeight};

/// The real shipped bytes: assets are product files, not fixtures, so
/// loading them honors the no-binary-fixtures rule.
fn repo_font_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/fonts"))
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
    // Collection indices per the noto-cjk OTC layout (jp,kr,sc,tc,hk).
    let cjk = [
        (8, "NotoSerifCJK-Regular.ttc", 2),
        (9, "NotoSerifCJK-Bold.ttc", 2),
        (10, "NotoSerifCJK-Regular.ttc", 3),
        (11, "NotoSerifCJK-Bold.ttc", 3),
        (12, "NotoSerifCJK-Regular.ttc", 0),
        (13, "NotoSerifCJK-Bold.ttc", 0),
        (14, "NotoSerifCJK-Regular.ttc", 1),
        (15, "NotoSerifCJK-Bold.ttc", 1),
        (16, "NotoSansCJK-Regular.ttc", 2),
        (17, "NotoSansCJK-Bold.ttc", 2),
        (18, "NotoSansCJK-Regular.ttc", 3),
        (19, "NotoSansCJK-Bold.ttc", 3),
        (20, "NotoSansCJK-Regular.ttc", 0),
        (21, "NotoSansCJK-Bold.ttc", 0),
        (22, "NotoSansCJK-Regular.ttc", 1),
        (23, "NotoSansCJK-Bold.ttc", 1),
    ];
    for (id, name, index) in cjk {
        assert_eq!(file(id), name, "id {id}");
        assert_eq!(entries[id].collection_index, index, "id {id}");
    }
    assert_eq!(file(24), "NotoSansSymbols2-Regular.ttf");
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
