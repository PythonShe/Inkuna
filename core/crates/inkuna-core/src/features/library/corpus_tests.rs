//! The digest's framing properties. Everything the restore guard rests on
//! is here: same corpus ⇒ same digest, any change to the character stream
//! ⇒ a different one.

use super::*;
use crate::Library;

/// `(spine_idx, text)` pairs in spine order, the shape both call sites
/// build.
fn digest(resources: &[(u32, Option<&str>)]) -> String {
    corpus_digest(resources.iter().copied())
}

#[test]
fn the_same_corpus_always_digests_the_same() {
    let corpus = [
        (0, Some("月の光が窓辺に落ちる。")),
        (1, None),
        (2, Some("second chapter")),
    ];
    assert_eq!(digest(&corpus), digest(&corpus));
    // Nothing about the digest depends on how the caller got there.
    let rebuilt: Vec<(u32, Option<&str>)> = corpus.to_vec();
    assert_eq!(digest(&corpus), corpus_digest(rebuilt));
}

#[test]
fn a_changed_character_changes_the_digest() {
    let before = digest(&[(0, Some("月の光が窓辺に落ちる。"))]);
    // One CJK character swapped — the kind of difference a converter or
    // entity-unescape change makes, and exactly the difference that
    // invalidates every offset after it.
    let after = digest(&[(0, Some("月の光が窓際に落ちる。"))]);
    assert_ne!(before, after);
    // As does adding one.
    assert_ne!(before, digest(&[(0, Some("月の光が窓辺に落ちる。 "))]));
}

/// The property length-prefixing exists for: the concatenated text is
/// byte-identical, but the coordinates mean something different, so the
/// digest must not collide.
#[test]
fn moving_text_between_resources_changes_the_digest() {
    let split_early = digest(&[(0, Some("月光")), (1, Some("書房"))]);
    let split_late = digest(&[(0, Some("月光書")), (1, Some("房"))]);
    let all_in_one = digest(&[(0, Some("月光書房")), (1, Some(""))]);
    assert_ne!(split_early, split_late);
    assert_ne!(split_early, all_in_one);
    assert_ne!(split_late, all_in_one);
}

#[test]
fn a_textless_resource_is_distinct_from_an_empty_one() {
    assert_ne!(digest(&[(0, None)]), digest(&[(0, Some(""))]));
}

#[test]
fn spine_order_and_length_are_part_of_the_digest() {
    let ordered = digest(&[(0, Some("alpha")), (1, Some("beta"))]);
    let swapped = digest(&[(0, Some("beta")), (1, Some("alpha"))]);
    assert_ne!(ordered, swapped, "which chapter holds which text matters");
    // A book that gained or lost a (textless) chapter is a different book
    // as far as coordinates are concerned.
    assert_ne!(
        ordered,
        digest(&[(0, Some("alpha")), (1, Some("beta")), (2, None)])
    );
}

#[test]
fn a_corpus_less_row_has_no_digest_to_compare() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::open(dir.path().join("library")).unwrap();
    let digest = library
        .readers
        .with(|conn| stored_corpus_digest(conn, "no-such-publication"))
        .unwrap();
    assert_eq!(
        digest, None,
        "no resources means no promise about what any coordinate addresses"
    );
}

/// The two entry points must agree, or the removal-side and restore-side
/// digests would never match and coordinates would degrade on every
/// restore.
#[test]
fn the_stored_digest_matches_the_in_memory_one() {
    use crate::test_support::{imported, write_epub};

    let dir = tempfile::tempdir().unwrap();
    let epub = dir.path().join("book.epub");
    write_epub(&epub, "月光書房", "紫式部", "ja");
    let library = Library::open(dir.path().join("library")).unwrap();
    let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

    let bodies: Vec<(u32, Option<String>)> = library
        .readers
        .with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT r.spine_idx, t.body FROM resources r
                 LEFT JOIN resource_text t ON t.resource_id = r.id
                 WHERE r.publication_id = ?1 ORDER BY r.spine_idx",
            )?;
            let rows = stmt.query_map([&publication.id], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect::<Result<_, _>>().map_err(Into::into)
        })
        .unwrap();
    assert!(!bodies.is_empty());

    let from_db = library
        .readers
        .with(|conn| stored_corpus_digest(conn, &publication.id))
        .unwrap();
    let from_memory = corpus_digest(bodies.iter().map(|(idx, body)| (*idx, body.as_deref())));
    assert_eq!(from_db, Some(from_memory));
}
