use std::path::Path;
use std::sync::Arc;

use super::super::registry::{FontRegistry, FIRST_DYNAMIC_ID};
use super::{SystemFontFace, SystemFontRole, SystemFontWarning};
use crate::settings::FontFamily;
use crate::style::{FontStyle, FontWeight};

/// The real shipped bytes stand in for platform system files: assets
/// are product files, not fixtures, so loading them honors the
/// no-binary-fixtures rule.
fn repo_font_dir() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../assets/fonts"
    ))
}

fn font_path(file: &str) -> String {
    repo_font_dir().join(file).to_string_lossy().into_owned()
}

fn face(role: SystemFontRole, italic: bool, weight: u16, file: &str) -> SystemFontFace {
    SystemFontFace {
        role,
        italic,
        weight,
        file_path: font_path(file),
        post_script_name: None,
        ttc_hint: None,
    }
}

fn load(faces: &[SystemFontFace]) -> (Arc<FontRegistry>, Vec<SystemFontWarning>) {
    match FontRegistry::load_with_system(repo_font_dir(), faces) {
        Ok(loaded) => loaded,
        Err(e) => panic!("repo font set must load: {e}"),
    }
}

#[test]
fn plain_load_has_empty_system_block() {
    let (reg, warnings) = load(&[]);
    assert!(warnings.is_empty());
    assert_eq!(reg.entries().len() as u32, FIRST_DYNAMIC_ID);
    assert_eq!(reg.next_free_id(), FIRST_DYNAMIC_ID);
}

/// Static faces take one id each, in registration order from
/// FIRST_DYNAMIC_ID, and serve their role's style+weight requests.
#[test]
fn static_faces_register_in_call_order() {
    // The static Hebrew files stand in for platform static faces.
    let (reg, warnings) = load(&[
        face(SystemFontRole::Serif, false, 400, "NotoSerifHebrew-Regular.ttf"),
        face(SystemFontRole::Serif, false, 700, "NotoSerifHebrew-Bold.ttf"),
    ]);
    assert!(warnings.is_empty(), "{warnings:?}");
    let entries = reg.entries();
    assert_eq!(entries.len() as u32, FIRST_DYNAMIC_ID + 2);
    assert_eq!(reg.next_free_id(), FIRST_DYNAMIC_ID + 2);

    let regular = &entries[FIRST_DYNAMIC_ID as usize];
    assert_eq!(regular.id, FIRST_DYNAMIC_ID);
    assert!(regular.axes.is_empty(), "static faces carry no axes");
    assert_eq!(regular.collection_index, 0);
    assert_eq!(regular.post_script_name, "NotoSerifHebrew-Regular");
    let bold = &entries[FIRST_DYNAMIC_ID as usize + 1];
    assert_eq!(bold.id, FIRST_DYNAMIC_ID + 1);
    assert_eq!(bold.post_script_name, "NotoSerifHebrew-Bold");

    // Selection: system serif serves upright weights via the nearest
    // rule; italic synthesizes from the uprights (none registered).
    let select = |style, weight: u16| {
        reg.select(FontFamily::SystemSerif, style, FontWeight::new(weight))
    };
    assert_eq!(select(FontStyle::Normal, 400), FIRST_DYNAMIC_ID);
    assert_eq!(select(FontStyle::Normal, 700), FIRST_DYNAMIC_ID + 1);
    assert_eq!(select(FontStyle::Normal, 100), FIRST_DYNAMIC_ID);
    assert_eq!(
        select(FontStyle::Normal, 500),
        FIRST_DYNAMIC_ID,
        "400..=500 checks toward 500 first, then below (400), then above"
    );
    assert_eq!(select(FontStyle::Normal, 550), FIRST_DYNAMIC_ID + 1, "over 500 goes up first");
    assert_eq!(select(FontStyle::Normal, 900), FIRST_DYNAMIC_ID + 1);
    assert_eq!(select(FontStyle::Italic, 400), FIRST_DYNAMIC_ID);
    assert_eq!(select(FontStyle::Italic, 700), FIRST_DYNAMIC_ID + 1);

    // The sans role has no system faces: it falls back to NotoSans.
    assert_eq!(
        reg.select(FontFamily::SystemSans, FontStyle::Normal, FontWeight::NORMAL),
        reg.select(FontFamily::NotoSans, FontStyle::Normal, FontWeight::NORMAL),
    );
}

/// A variable face registers the nine standard weights as wght
/// instances, ascending, each on its own consecutive id.
#[test]
fn variable_face_instances_nine_weights() {
    let (reg, warnings) = load(&[face(
        SystemFontRole::Sans,
        false,
        400,
        "NotoSerif.ttf", // variable wght 100..=900; role is caller-declared
    )]);
    assert!(warnings.is_empty(), "{warnings:?}");
    let entries = reg.entries();
    assert_eq!(entries.len() as u32, FIRST_DYNAMIC_ID + 9);
    for (slot, weight) in [100u16, 200, 300, 400, 500, 600, 700, 800, 900]
        .into_iter()
        .enumerate()
    {
        let entry = &entries[FIRST_DYNAMIC_ID as usize + slot];
        assert_eq!(entry.id, FIRST_DYNAMIC_ID + slot as u32);
        assert_eq!(entry.post_script_name, "NotoSerif-Regular");
        assert_eq!(entry.axes.len(), 1, "one wght coordinate per instance");
        assert_eq!(entry.axes[0].tag, "wght");
        assert_eq!(entry.axes[0].value, f64::from(weight));
        // Instances reuse the base face's default-instance metrics.
        let instance = reg.face(entry.id);
        let base = reg.face(0);
        assert_eq!(instance.upem, base.upem);
        assert_eq!(instance.ascender, base.ascender);
        assert_eq!(instance.descender, base.descender);
    }
    // Numeric selection over the instance list: exact hits, then the
    // CSS nearest rule.
    let sans = |weight: u16| {
        reg.select(FontFamily::SystemSans, FontStyle::Normal, FontWeight::new(weight))
    };
    assert_eq!(sans(400), FIRST_DYNAMIC_ID + 3);
    assert_eq!(sans(700), FIRST_DYNAMIC_ID + 6);
    assert_eq!(sans(350), FIRST_DYNAMIC_ID + 2, "below 400 goes down first");
    assert_eq!(sans(450), FIRST_DYNAMIC_ID + 4, "400..=500 checks toward 500 first");
    assert_eq!(sans(950), FIRST_DYNAMIC_ID + 8, "clamps down to 900");
    // The serif role stays bundled Noto.
    assert_eq!(
        reg.select(FontFamily::SystemSerif, FontStyle::Normal, FontWeight::NORMAL),
        0
    );
}

/// PostScript-name matching scans the collection indices — the OTC
/// stand-in proves a non-zero index is found and recorded.
#[test]
fn post_script_name_scans_collection_indices() {
    let mut tc = face(SystemFontRole::Serif, false, 400, "NotoSerifCJK-Regular.ttc");
    tc.post_script_name = Some("NotoSerifCJKtc-Regular".to_string());
    let mut jp = face(SystemFontRole::Serif, false, 700, "NotoSerifCJK-Regular.ttc");
    jp.post_script_name = Some("NotoSerifCJKjp-Regular".to_string());
    let (reg, warnings) = load(&[tc, jp]);
    assert!(warnings.is_empty(), "{warnings:?}");
    let entries = reg.entries();
    let first = &entries[FIRST_DYNAMIC_ID as usize];
    // The noto-cjk OTC region order is jp, kr, sc, tc, hk.
    assert_eq!(first.collection_index, 3, "tc face sits at index 3");
    assert_eq!(first.post_script_name, "NotoSerifCJKtc-Regular");
    let second = &entries[FIRST_DYNAMIC_ID as usize + 1];
    assert_eq!(second.collection_index, 0, "jp face sits at index 0");
    assert_eq!(second.post_script_name, "NotoSerifCJKjp-Regular");
    // Both ids share one mmap of the .ttc.
    assert!(Arc::ptr_eq(
        &reg.face(first.id).data,
        &reg.face(second.id).data
    ));
}

/// Without a PostScript name, `ttc_hint` picks the face directly.
#[test]
fn ttc_hint_picks_collection_index() {
    let mut kr = face(SystemFontRole::Serif, false, 400, "NotoSerifCJK-Regular.ttc");
    kr.ttc_hint = Some(1);
    let (reg, warnings) = load(&[kr]);
    assert!(warnings.is_empty(), "{warnings:?}");
    let entry = &reg.entries()[FIRST_DYNAMIC_ID as usize];
    assert_eq!(entry.collection_index, 1);
    assert_eq!(entry.post_script_name, "NotoSerifCJKkr-Regular");
}

/// Failures degrade: bad faces are skipped with warnings, take no ids,
/// and later registrations still land in call order.
#[test]
fn failed_faces_warn_skip_and_keep_ids_sequential() {
    let mut bad_name = face(SystemFontRole::Serif, false, 400, "NotoSerifCJK-Regular.ttc");
    bad_name.post_script_name = Some("NoSuchFace-Regular".to_string());
    let (reg, warnings) = load(&[
        face(SystemFontRole::Serif, false, 400, "no-such-file.ttf"),
        bad_name,
        face(SystemFontRole::Serif, false, 400, "NotoSerifHebrew-Regular.ttf"),
    ]);
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings[0].file_path.ends_with("no-such-file.ttf"));
    assert!(warnings[1].detail.contains("NoSuchFace-Regular"));
    // The good face took the FIRST dynamic id — failures allocate none.
    let entries = reg.entries();
    assert_eq!(entries.len() as u32, FIRST_DYNAMIC_ID + 1);
    assert_eq!(
        entries[FIRST_DYNAMIC_ID as usize].post_script_name,
        "NotoSerifHebrew-Regular"
    );
    assert_eq!(
        reg.select(FontFamily::SystemSerif, FontStyle::Normal, FontWeight::NORMAL),
        FIRST_DYNAMIC_ID
    );
}

/// An italic-only registration does NOT hijack the role: body text must
/// never render italic, so the whole role falls back to Noto.
#[test]
fn italic_only_role_falls_back_entirely() {
    let (reg, warnings) = load(&[face(
        SystemFontRole::Serif,
        true,
        400,
        "NotoSerifHebrew-Regular.ttf",
    )]);
    assert!(warnings.is_empty(), "{warnings:?}");
    for style in [FontStyle::Normal, FontStyle::Italic] {
        assert_eq!(
            reg.select(FontFamily::SystemSerif, style, FontWeight::NORMAL),
            reg.select(FontFamily::NotoSerif, style, FontWeight::NORMAL),
        );
    }
}

/// With no registered faces, System*/Publisher requests resolve to the
/// exact same ids as their Noto equivalents — identical layout, so a
/// settings switch between them costs a re-layout but never differs.
#[test]
fn unregistered_families_resolve_to_noto_ids() {
    let (reg, _) = load(&[]);
    for style in [FontStyle::Normal, FontStyle::Italic] {
        for weight in [100u16, 400, 550, 700, 900] {
            let w = FontWeight::new(weight);
            assert_eq!(
                reg.select(FontFamily::SystemSerif, style, w),
                reg.select(FontFamily::NotoSerif, style, w),
                "serif {style:?} {weight}"
            );
            assert_eq!(
                reg.select(FontFamily::SystemSans, style, w),
                reg.select(FontFamily::NotoSans, style, w),
                "sans {style:?} {weight}"
            );
            // Publisher's explicit B1 fallback is NotoSerif.
            assert_eq!(
                reg.select(FontFamily::Publisher, style, w),
                reg.select(FontFamily::NotoSerif, style, w),
                "publisher {style:?} {weight}"
            );
        }
    }
}

/// The registered-system selection path honors italic faces when they
/// exist: italic requests pick the italic slot, uprights the upright.
#[test]
fn italic_slot_serves_italic_requests() {
    let (reg, warnings) = load(&[
        face(SystemFontRole::Serif, false, 400, "NotoSerifHebrew-Regular.ttf"),
        face(SystemFontRole::Serif, true, 400, "NotoSerifHebrew-Bold.ttf"), // stand-in "italic"
    ]);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        reg.select(FontFamily::SystemSerif, FontStyle::Normal, FontWeight::NORMAL),
        FIRST_DYNAMIC_ID
    );
    assert_eq!(
        reg.select(FontFamily::SystemSerif, FontStyle::Italic, FontWeight::NORMAL),
        FIRST_DYNAMIC_ID + 1
    );
}
