use std::sync::Arc;

use super::{ActiveSlot, Bookshelf};
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

/// Sequential opens: each request vacates the slot (handing back the
/// previous session to close) and installs cleanly when nothing newer
/// was requested meanwhile.
#[test]
fn active_slot_sequential_opens_hand_back_the_previous_session() {
    let slot: ActiveSlot<u32> = ActiveSlot::new();
    let (first_ticket, previous) = slot.begin();
    assert!(previous.is_none(), "fresh slot must be empty");
    let first = Arc::new(1u32);
    assert!(
        slot.store(first_ticket, &first).is_none(),
        "nothing to displace on the first install"
    );
    let (second_ticket, previous) = slot.begin();
    assert!(
        previous.is_some_and(|s| Arc::ptr_eq(&s, &first)),
        "the next request must get the live previous session to close"
    );
    let second = Arc::new(2u32);
    assert!(slot.store(second_ticket, &second).is_none());
}

/// The X1 race: a slow OLDER open finishing after a newer one must not
/// displace the newer, visible session — it gets itself back to close
/// off to the side, and the newer session keeps the slot.
#[test]
fn active_slot_orders_concurrent_opens_by_request_not_completion() {
    let slot: ActiveSlot<u32> = ActiveSlot::new();
    let (older_ticket, _) = slot.begin();
    let (newer_ticket, _) = slot.begin();

    // The newer request completes FIRST and installs.
    let newer = Arc::new(2u32);
    assert!(slot.store(newer_ticket, &newer).is_none());

    // The older request completes LAST: it must close itself, not the
    // newer session.
    let older = Arc::new(1u32);
    let stale = slot.store(older_ticket, &older);
    assert!(
        stale.is_some_and(|s| Arc::ptr_eq(&s, &older)),
        "a stale open must get its own session back to close"
    );

    // The newer session still holds the slot for the next request.
    let (_, previous) = slot.begin();
    assert!(
        previous.is_some_and(|s| Arc::ptr_eq(&s, &newer)),
        "the newer session must have kept the slot"
    );
}

/// A newer request that is still opening (begun, not yet stored) also
/// beats an older completion — the slot stays vacant for it.
#[test]
fn active_slot_stale_store_defers_to_a_newer_open_still_in_flight() {
    let slot: ActiveSlot<u32> = ActiveSlot::new();
    let (older_ticket, _) = slot.begin();
    let (newer_ticket, _) = slot.begin();

    // The older request completes while the newer is still opening: it
    // must stand aside rather than install into the vacated slot.
    let older = Arc::new(1u32);
    assert!(slot.store(older_ticket, &older).is_some());

    // The newer request then installs into an empty slot.
    let newer = Arc::new(2u32);
    assert!(slot.store(newer_ticket, &newer).is_none());
    let (_, previous) = slot.begin();
    assert!(previous.is_some_and(|s| Arc::ptr_eq(&s, &newer)));
}
