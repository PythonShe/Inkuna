use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::PublisherFaceSpec;
use crate::fonts::FontRegistry;
use crate::settings::FontFamily;
use crate::style::{FamilyName, FontStyle, FontWeight};

/// The real shipped bytes: assets are product files, not fixtures.
fn repo_font(file: &str) -> PathBuf {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../assets/fonts"
    ))
    .join(file)
}

fn base() -> Arc<FontRegistry> {
    let dir = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../assets/fonts"
    ));
    FontRegistry::load(dir).unwrap_or_else(|e| panic!("repo font set must load: {e}"))
}

fn named(name: &str) -> FamilyName {
    FamilyName::Named(name.to_string())
}

/// Variable face (NotoSerif.ttf, wght 100–900) declared over the full
/// range, plus a static face (Hebrew Regular) declared (300, 700).
fn specs() -> Vec<PublisherFaceSpec> {
    vec![
        PublisherFaceSpec {
            file_path: repo_font("NotoSerif.ttf"),
            family: "Pub Serif".to_string(),
            italic: false,
            weight: (100, 900),
        },
        PublisherFaceSpec {
            file_path: repo_font("NotoSerifHebrew-Regular.ttf"),
            family: "Pub Static".to_string(),
            italic: false,
            weight: (300, 700),
        },
    ]
}

#[test]
fn publisher_block_appends_deterministically_without_touching_base() {
    let base = base();
    let first_free = base.next_free_id();
    let derived = FontRegistry::with_publisher(&base, &specs());

    // Variable face: nine wght instances; static: one id.
    assert_eq!(derived.next_free_id(), first_free + 9 + 1);
    // The base registry is untouched: derived tables are per-session.
    assert_eq!(base.next_free_id(), first_free);

    let entries = derived.entries();
    let block = &entries[first_free as usize..];
    for (at, entry) in block.iter().enumerate() {
        assert_eq!(entry.id, first_free + at as u32);
    }
    // Instances carry ascending wght axes 100..=900.
    let weights: Vec<f64> = block[..9]
        .iter()
        .map(|e| e.axes.first().map(|a| a.value).unwrap_or(0.0))
        .collect();
    assert_eq!(
        weights,
        vec![100.0, 200.0, 300.0, 400.0, 500.0, 600.0, 700.0, 800.0, 900.0]
    );
    assert!(block[9].axes.is_empty(), "static face has no axes");

    // Same base + same specs → identical block (determinism).
    let again = FontRegistry::with_publisher(&base, &specs());
    assert_eq!(again.entries(), entries);
}

#[test]
fn select_stack_matches_names_case_insensitively_and_by_weight() {
    let base = base();
    let first_free = base.next_free_id();
    let derived = FontRegistry::with_publisher(&base, &specs());

    // Weight 400 hits the wght=400 instance (fourth of the block).
    let id = derived.select_stack(
        &[named("pub serif")],
        FontStyle::Normal,
        FontWeight::NORMAL,
    );
    assert_eq!(id, first_free + 3);
    // 620 is not a standard stop: css nearest goes up to 700.
    let id = derived.select_stack(
        &[named("PUB SERIF")],
        FontStyle::Normal,
        FontWeight::new(620),
    );
    assert_eq!(id, first_free + 6);

    // The static face serves its whole declared range and the nearest
    // rule outside it; an italic request synthesizes from the upright.
    let static_id = first_free + 9;
    for weight in [300, 500, 700, 900, 100] {
        assert_eq!(
            derived.select_stack(
                &[named("Pub Static")],
                FontStyle::Normal,
                FontWeight::new(weight)
            ),
            static_id
        );
    }
    assert_eq!(
        derived.select_stack(&[named("Pub Static")], FontStyle::Italic, FontWeight::NORMAL),
        static_id
    );
}

#[test]
fn select_stack_walks_and_falls_back() {
    let base = base();
    let derived = FontRegistry::with_publisher(&base, &specs());
    let noto_serif = derived.select(FontFamily::NotoSerif, FontStyle::Normal, FontWeight::NORMAL);
    let noto_sans = derived.select(FontFamily::NotoSans, FontStyle::Normal, FontWeight::NORMAL);

    // Unknown name walks on to the generic.
    assert_eq!(
        derived.select_stack(
            &[named("No Such Family"), FamilyName::SansSerif],
            FontStyle::Normal,
            FontWeight::NORMAL
        ),
        noto_sans
    );
    // Monospace (no bundled roster) walks on to a publisher match.
    assert_eq!(
        derived.select_stack(
            &[FamilyName::Monospace, named("Pub Static")],
            FontStyle::Normal,
            FontWeight::NORMAL
        ),
        base.next_free_id() + 9
    );
    // Nothing matches → NotoSerif; explicit serif generic → NotoSerif.
    assert_eq!(
        derived.select_stack(&[named("Ghost")], FontStyle::Normal, FontWeight::NORMAL),
        noto_serif
    );
    assert_eq!(
        derived.select_stack(&[FamilyName::Serif], FontStyle::Normal, FontWeight::NORMAL),
        noto_serif
    );
    // The empty stack (no font-family anywhere) is NotoSerif too.
    assert_eq!(
        derived.select_stack(&[], FontStyle::Normal, FontWeight::NORMAL),
        noto_serif
    );

    // A bare Publisher request never resolves to a publisher face —
    // stacks are the only path in.
    assert_eq!(
        derived.select(FontFamily::Publisher, FontStyle::Normal, FontWeight::NORMAL),
        noto_serif
    );
}

/// A spec whose file is missing is skipped with a warning — later specs
/// still register, at the ids the loadable list dictates.
#[test]
fn unloadable_specs_skip_without_failing() {
    let base = base();
    let first_free = base.next_free_id();
    let mut specs = specs();
    specs.insert(
        0,
        PublisherFaceSpec {
            file_path: repo_font("no-such-font.ttf"),
            family: "Ghost".to_string(),
            italic: false,
            weight: (400, 400),
        },
    );
    let derived = FontRegistry::with_publisher(&base, &specs);
    assert_eq!(derived.next_free_id(), first_free + 10);
    assert_eq!(
        derived.select_stack(&[named("Ghost")], FontStyle::Normal, FontWeight::NORMAL),
        derived.select(FontFamily::NotoSerif, FontStyle::Normal, FontWeight::NORMAL),
        "the ghost family fell back"
    );
}
