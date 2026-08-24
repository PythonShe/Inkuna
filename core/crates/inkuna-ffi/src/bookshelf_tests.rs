use super::Bookshelf;
use crate::error::InkunaError;
use crate::fonts::{SystemFontFace, SystemFontRole};

/// The real shipped bytes stand in for platform system files.
fn repo_font_dir() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/fonts").to_string()
}

fn open_shelf(data_dir: &std::path::Path) -> std::sync::Arc<Bookshelf> {
    match Bookshelf::open(
        data_dir.to_string_lossy().into_owned(),
        repo_font_dir(),
    ) {
        Ok(shelf) => shelf,
        Err(e) => panic!("bookshelf must open: {e}"),
    }
}

fn hebrew_face() -> SystemFontFace {
    SystemFontFace {
        role: SystemFontRole::Serif,
        italic: false,
        weight: 400,
        file_path: format!("{}/NotoSerifHebrew-Regular.ttf", repo_font_dir()),
        post_script_name: Some("NotoSerifHebrew-Regular".to_string()),
        ttc_hint: None,
    }
}

/// Registration loads the registry once; a second call must be
/// rejected — ids are allocated exactly once per process.
#[tokio::test]
async fn register_system_fonts_is_once_only() {
    let dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => panic!("tempdir: {e}"),
    };
    let shelf = open_shelf(dir.path());
    let warnings = match shelf.register_system_fonts(vec![hebrew_face()]).await {
        Ok(w) => w,
        Err(e) => panic!("first registration must succeed: {e}"),
    };
    assert!(warnings.is_empty(), "{warnings:?}");
    match shelf.register_system_fonts(vec![hebrew_face()]).await {
        Err(InkunaError::InvalidState { detail }) => {
            assert!(detail.contains("already loaded"), "{detail}");
        }
        Ok(_) => panic!("second registration must be rejected"),
        Err(e) => panic!("wrong error kind: {e}"),
    }
}

/// A face that fails to load is a warning, never an error — the
/// registry still comes up on the bundled set.
#[tokio::test]
async fn register_system_fonts_degrades_to_warnings() {
    let dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => panic!("tempdir: {e}"),
    };
    let shelf = open_shelf(dir.path());
    let mut missing = hebrew_face();
    missing.file_path = format!("{}/no-such-face.ttf", repo_font_dir());
    missing.post_script_name = None;
    let warnings = match shelf.register_system_fonts(vec![missing]).await {
        Ok(w) => w,
        Err(e) => panic!("registration must degrade, not fail: {e}"),
    };
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].file_path.ends_with("no-such-face.ttf"));
}
