//! Corpus provenance: the digest that decides whether coordinates frozen
//! across a removal still address the same characters when the book comes
//! back.
//!
//! A stored coordinate is a `(spine_idx, char_offset)` index into the
//! canonical text projection, and nothing about that projection is pinned
//! by the thing re-import matches on: `publications.content_hash` is the
//! hash of the *pre-conversion* source bytes, so a re-imported MOBI, AZW3,
//! or TXT is converted and projected again by whatever build is running
//! now. Better chapter detection, a fixed entity unescape, a different
//! aggregate text budget — any of them silently yields a different
//! character stream from byte-identical input.
//!
//! So the guard is mechanical rather than declarative: removal digests the
//! corpus it is about to delete, restore digests the corpus it just built,
//! and the coordinates reattach only if the two agree. Nobody has to
//! remember to bump anything.

use rusqlite::types::ValueRef;
use rusqlite::Connection;

use crate::CoreError;

/// Domain separator, and the digest scheme's own version. It prefixes
/// every digest, so changing the framing below (and bumping this) makes
/// every older digest compare unequal — which degrades coordinates, the
/// fail-safe direction.
const DIGEST_TAG: &[u8] = b"inkuna/corpus-digest/1\0";

/// BLAKE3 over the canonical corpus, as lowercase hex.
///
/// `resources` must arrive in ascending `spine_idx` order — the same order
/// the coordinates index. Determinism comes from framing rather than from
/// the caller: each resource contributes its spine index (4 bytes, little
/// endian), a present/absent tag byte, and, when present, its body's byte
/// length (8 bytes, little endian) before the body itself, with the
/// resource count appended at the end. Length-prefixing is what makes the
/// concatenation unambiguous: moving a character from one resource to the
/// next changes two lengths and therefore the digest, even though the
/// concatenated text is identical. No map iteration, no timestamps, no
/// locale-dependent formatting — the same sequence always hashes the same
/// way, on any platform.
///
/// Hrefs are deliberately *not* hashed. A coordinate addresses
/// `(spine_idx, char_offset)`, so renaming a resource inside the container
/// cannot move a single character; folding hrefs in would degrade
/// coordinates for changes that cannot affect them.
pub(crate) fn corpus_digest<'a, I>(resources: I) -> String
where
    I: IntoIterator<Item = (u32, Option<&'a str>)>,
{
    let mut hasher = blake3::Hasher::new();
    hasher.update(DIGEST_TAG);
    let mut count: u64 = 0;
    for (spine_idx, body) in resources {
        count += 1;
        hash_resource(&mut hasher, spine_idx, body.map(str::as_bytes));
    }
    hasher.update(&count.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

/// The digest of the corpus currently stored for `publication_id`, or
/// `None` when it has no spine resources at all.
///
/// `None` is unknown provenance, and every consumer treats that exactly
/// like a mismatch: a row with no corpus to digest can make no promise
/// about what its coordinates address.
///
/// Streams the bodies one at a time straight into the hasher rather than
/// collecting them: the whole corpus is bounded by `MAX_TOTAL_TEXT_BYTES`,
/// but there is no reason to hold a book's full text in memory to delete
/// it.
pub(crate) fn stored_corpus_digest(
    conn: &Connection,
    publication_id: &str,
) -> Result<Option<String>, CoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.spine_idx, t.body FROM resources r
         LEFT JOIN resource_text t ON t.resource_id = r.id
         WHERE r.publication_id = ?1 ORDER BY r.spine_idx",
    )?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(DIGEST_TAG);
    let mut count: u64 = 0;
    let mut rows = stmt.query([publication_id])?;
    while let Some(row) = rows.next()? {
        count += 1;
        let spine_idx: u32 = row.get(0)?;
        // Hashed straight out of SQLite's own buffer: `get::<String>`
        // would copy every chapter through the heap for nothing.
        let body = match row.get_ref(1)? {
            ValueRef::Null => None,
            ValueRef::Text(bytes) | ValueRef::Blob(bytes) => Some(bytes),
            // `resource_text.body` is a TEXT column only this crate ever
            // writes, so the numeric variants are unreachable. Digesting
            // them as empty keeps the match total without inventing a
            // conversion error, and it can only ever *fail* to match —
            // which degrades coordinates, the safe direction.
            ValueRef::Integer(_) | ValueRef::Real(_) => Some(&b""[..]),
        };
        hash_resource(&mut hasher, spine_idx, body);
    }
    if count == 0 {
        return Ok(None);
    }
    hasher.update(&count.to_le_bytes());
    Ok(Some(hasher.finalize().to_hex().to_string()))
}

/// One resource's contribution, shared by both entry points so the two can
/// never drift apart.
fn hash_resource(hasher: &mut blake3::Hasher, spine_idx: u32, body: Option<&[u8]>) {
    hasher.update(&spine_idx.to_le_bytes());
    match body {
        None => {
            hasher.update(&[0u8]);
        }
        Some(body) => {
            hasher.update(&[1u8]);
            hasher.update(&(body.len() as u64).to_le_bytes());
            hasher.update(body);
        }
    }
}

#[cfg(test)]
#[path = "corpus_tests.rs"]
mod tests;
