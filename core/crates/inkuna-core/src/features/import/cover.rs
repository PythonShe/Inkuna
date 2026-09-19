//! Cover normalization: covers are downscaled and re-encoded to lossy
//! WebP at import, so `covers/` only ever holds display-sized files and
//! the shells can decode any cover without pathological allocations. A
//! cover that cannot be decoded (SVG, exotic codecs, corrupt data) passes
//! through untouched — a cover is optional data and never fails an
//! import.
//!
//! The background pass at the bottom keeps stored covers in agreement with
//! that: it re-encodes what older versions left behind, and rebuilds a
//! cover whose file has gone missing from the book itself. Both write to
//! `covers/<id>.<ext>`, which — because a removed book's id can be
//! reclaimed by a restore — is shared property, not this pass's own. So it
//! borrows `commit`'s discipline wholesale: stage under a unique name,
//! claim the row under the writer lock, place the file, commit.

use std::io::Cursor;
use std::path::Path;
use std::sync::Mutex;

use image::imageops::FilterType;
use image::ImageReader;

use crate::formats::epub::{self, Cover};
use crate::{CoreError, Library};

/// Bounding box the stored cover must fit inside, preserving aspect
/// ratio. The largest cover either shell draws today is 150 pt/dp; at 3×
/// that is 450 px, so 600×900 covers every current surface with headroom
/// while keeping a photographic cover around a few tens of kilobytes.
const MAX_COVER_WIDTH: u32 = 600;
const MAX_COVER_HEIGHT: u32 = 900;

/// Ceiling on the *decoded* pixel count, checked against the header
/// before any pixel is allocated. The 16 MiB compressed cap upstream
/// (`epub::archive`) does not bound decoded size — 16 MiB of PNG can
/// inflate to gigabytes — so a source past this cap is stored as-is and
/// left to the shells' own sampled decoders. 24 MP (≈4000×6000, ~96 MB
/// as RGBA8) admits any plausible real cover while keeping the worst
/// admitted decode phone-sized.
const MAX_DECODE_PIXELS: u64 = 24_000_000;

/// One decode+encode at a time, process-wide. Import batches fan out on
/// rayon and the legacy re-encode pass runs at startup; without this
/// gate, peak transient memory scales with worker count — up to
/// [`MAX_DECODE_PIXELS`]-sized RGBA buffers per thread — exactly when
/// Android's low-memory killer and iOS jetsam are least forgiving.
/// Covers are a small slice of import cost, so serializing them trades
/// nothing measurable for a bounded peak.
static DECODE_GATE: Mutex<()> = Mutex::new(());

/// Lossy WebP quality. 80 is visually transparent at thumbnail sizes and
/// roughly a quarter of an equivalent JPEG's bytes.
const WEBP_QUALITY: f32 = 80.0;

/// The cover as import will persist it: the normalized WebP when the
/// source could be decoded, the original bytes otherwise.
pub(super) fn normalize_cover(cover: Cover) -> Cover {
    normalized(&cover.bytes, &cover.extension).unwrap_or(cover)
}

/// Downscales `bytes` into the cover bounding box and re-encodes as lossy
/// WebP. `None` means pass through: undecodable input, a decoded size
/// past [`MAX_DECODE_PIXELS`], or a source already within bounds whose
/// re-encode would not be smaller.
fn normalized(bytes: &[u8], extension: &str) -> Option<Cover> {
    let (width, height) = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_DECODE_PIXELS {
        return None;
    }
    let within = width <= MAX_COVER_WIDTH && height <= MAX_COVER_HEIGHT;
    if within && extension == "webp" {
        // Already the target format at display size: re-encoding could
        // only stack generation loss.
        return None;
    }
    // The gate guards no data, only concurrency, so a panic while a slot
    // was held poisons nothing worth propagating: recover and carry on.
    let _decode_slot = DECODE_GATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let decoded = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    let decoded = if within {
        decoded
    } else {
        decoded.resize(MAX_COVER_WIDTH, MAX_COVER_HEIGHT, FilterType::Lanczos3)
    };
    let rgba = decoded.into_rgba8();
    let (out_width, out_height) = rgba.dimensions();
    let encoded = webp::Encoder::from_rgba(rgba.as_raw(), out_width, out_height)
        .encode_simple(false, WEBP_QUALITY)
        .ok()?;
    if within && encoded.len() >= bytes.len() {
        // No downscale happened and WebP lost the size race (tiny or
        // already hyper-optimized sources): keep the original.
        return None;
    }
    Some(Cover {
        bytes: encoded.to_vec(),
        extension: "webp".into(),
    })
}

/// One live row's cover as the pass's worklist snapshot has it. The
/// snapshot can go stale in every field — the row may be removed, or
/// removed and restored onto the same id, while the bytes are being
/// re-encoded — so nothing here is trusted beyond being the token the row
/// is claimed with under the writer lock.
struct CoverRow<'a> {
    id: &'a str,
    /// `cover_path`: the file to refresh, and half the claim.
    rel: &'a str,
    /// `file_path`: the book the cover is re-extracted from when the file
    /// at `rel` has gone missing.
    book_rel: &'a str,
}

impl Library {
    /// Brings every live row's cover back into agreement with what the
    /// library should hold, returning how many rows it rewrote:
    ///
    /// - a cover persisted before normalization existed (or by an older
    ///   version) is re-encoded into the bounded WebP form;
    /// - a cover whose *file* has gone missing is re-extracted from the
    ///   stored book — the same extraction import runs — so a row is never
    ///   left pointing at art nothing can produce again;
    /// - a row whose cover can no longer be produced at all (its book is
    ///   missing, unreadable, or simply carries no cover art) stops
    ///   claiming one: `cover_path` is cleared, which is both honest and
    ///   what keeps the re-extraction above from running on every open
    ///   forever. Its cover comes back with a remove + re-import, which is
    ///   the only thing that can rebuild it anyway.
    ///
    /// Idempotent — an already-normalized cover whose file is there is a
    /// fixed point — and per-cover failures are logged and skipped rather
    /// than failing the pass: a cover is derived data. Safe to run in the
    /// background at any time after open, which is exactly what both
    /// shells do.
    pub fn optimize_covers(&self) -> Result<u32, CoreError> {
        self.optimize_covers_hooked(&|| {})
    }

    /// [`optimize_covers`](Self::optimize_covers) with a hook fired in the
    /// one window this pass cannot close from the inside: a replacement
    /// cover staged under its temp name, the writer lock not yet taken.
    /// The production path passes a no-op; tests pass a closure that drives
    /// a removal and a restore into exactly that gap. Mirrors
    /// `commit_import_hooked`.
    fn optimize_covers_hooked(&self, before_lock: &dyn Fn()) -> Result<u32, CoreError> {
        let rows: Vec<(String, String, String)> = self.readers.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, cover_path, file_path FROM publications_all
                     WHERE cover_path IS NOT NULL AND removed_at IS NULL",
            )?;
            let rows = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })?;

        // Sequential on purpose: the decode dominates and [`DECODE_GATE`]
        // serializes it anyway, so fanning out on rayon would only pin
        // worker threads against the gate during app startup.
        let mut changed = 0;
        for (id, rel, book_rel) in &rows {
            let row = CoverRow { id, rel, book_rel };
            match self.refresh_cover(&row, before_lock) {
                Ok(true) => changed += 1,
                Ok(false) => {}
                Err(error) => log::warn!("cover optimization skipped for {id}: {error}"),
            }
        }
        Ok(changed)
    }

    /// One row's refresh: re-encode what is on disk, or — when the file is
    /// gone — rebuild it from the book, or give the claim up.
    fn refresh_cover(&self, row: &CoverRow, before_lock: &dyn Fn()) -> Result<bool, CoreError> {
        match std::fs::read(self.data_dir.join(row.rel)) {
            Ok(bytes) => {
                let extension = row.rel.rsplit_once('.').map(|(_, s)| s).unwrap_or("");
                match normalized(&bytes, extension) {
                    Some(cover) => self.place_cover(row, &cover, before_lock),
                    None => Ok(false),
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match self.extracted_cover(row) {
                    Some(cover) => self.place_cover(row, &cover, before_lock),
                    None => self.clear_cover(row),
                }
            }
            // Anything else (a permission or I/O failure) is not a
            // statement about the cover: leave the row alone and let the
            // next pass try again.
            Err(error) => Err(error.into()),
        }
    }

    /// The cover art the stored book carries, normalized exactly as import
    /// normalizes it — the same `read_package` the import pipeline parses
    /// with, so a re-extracted cover is byte-identical to the one that
    /// import would have written. `None` (logged) whenever the book cannot
    /// produce one: the caller then clears the row's claim rather than
    /// retrying this parse on every open.
    fn extracted_cover(&self, row: &CoverRow) -> Option<Cover> {
        match epub::read_package(&self.data_dir.join(row.book_rel)) {
            Ok(parsed) => parsed.cover.map(normalize_cover),
            Err(error) => {
                log::warn!("cover re-extraction failed for {}: {error}", row.id);
                None
            }
        }
    }

    /// Stages `cover` under a name no other actor can spell, then hands it
    /// to [`claim_and_place`](Self::claim_and_place).
    ///
    /// Staging is the whole point. `covers/<id>.<ext>` is *not* this pass's
    /// property: a removed book's id can be reclaimed, so that path is also
    /// where `Library::remove` unlinks and where a restore of the same
    /// content places its own cover. Writing there before the row is
    /// claimed is what let this pass overwrite — or, when its guarded
    /// update then missed, delete — a restored live row's only cover, which
    /// nothing heals (the open-time sweep collects only *unreferenced*
    /// files). So nothing touches the shared path until the row is claimed,
    /// and the temp, which is ours alone, goes either way.
    fn place_cover(
        &self,
        row: &CoverRow,
        cover: &Cover,
        before_lock: &dyn Fn(),
    ) -> Result<bool, CoreError> {
        let new_rel = format!("covers/{}.{}", row.id, cover.extension);
        let staged = self
            .data_dir
            .join(format!("covers/{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&staged, &cover.bytes).inspect_err(|_| {
            let _ = std::fs::remove_file(&staged);
        })?;

        // The one window a concurrent removal or restore can occupy: the
        // cover is staged and the lock is not yet held.
        before_lock();

        let placed = self.claim_and_place(row, &new_rel, &staged);
        // Renamed away on success; on every other path this is what
        // withdrawing the re-encode means — including when the new name
        // equals the old one, which the pre-claim rename used to leave
        // orphaned.
        let _ = std::fs::remove_file(&staged);
        placed
    }

    /// Claims the row and only then moves the staged cover onto it, both
    /// under the writer lock.
    ///
    /// That ordering is what composes with `Library::remove`, which claims
    /// the live row and then unlinks its files while still holding this
    /// same lock: a removal is therefore strictly before this claim (which
    /// then finds a tombstone and matches nothing) or strictly after this
    /// commit — never between the claim and the rename. The old file is
    /// unlinked under the lock for the same reason, after the commit that
    /// made it unreferenced.
    ///
    /// The claim is exact about liveness and about the path it replaces,
    /// which is what a reclaimed id needs; it cannot, however, tell a row
    /// that kept its `cover_path` spelling through a remove and a restore
    /// apart from the row this pass read. It does not have to: a restore
    /// only ever adopts an id on a *content-hash* match, so such a row
    /// holds the same book, and what lands on it is a normalization of
    /// that same book's cover — never another book's art, and never a
    /// reference to a file that is not there.
    fn claim_and_place(
        &self,
        row: &CoverRow,
        new_rel: &str,
        staged: &Path,
    ) -> Result<bool, CoreError> {
        let mut conn = self.writer.lock().unwrap();
        let tx = conn.transaction()?;
        if claim(&tx, row, Some(new_rel))? == 0 {
            return Ok(false);
        }
        // A failure here rolls the claim back with the dropped transaction,
        // so the row keeps pointing at the file it already has.
        std::fs::rename(staged, self.data_dir.join(new_rel))?;
        tx.commit()?;
        if new_rel != row.rel {
            let _ = std::fs::remove_file(self.data_dir.join(row.rel));
        }
        Ok(true)
    }

    /// Drops a row's claim on a cover nothing can rebuild. Pure row work —
    /// the file it named is already gone — under the same guarded claim, so
    /// a row that was removed or restored meanwhile keeps whatever the
    /// winner gave it.
    fn clear_cover(&self, row: &CoverRow) -> Result<bool, CoreError> {
        let conn = self.writer.lock().unwrap();
        Ok(claim(&conn, row, None)? == 1)
    }
}

/// Points `row`'s publication at `cover_path` — the file just placed, or
/// NULL when the pass gave the claim up — but only while it is still the
/// live row this pass read: same id, same `cover_path`, not a tombstone.
/// Returns how many rows matched (0 or 1); 0 means the row moved under us
/// and this pass owns nothing.
fn claim(
    conn: &rusqlite::Connection,
    row: &CoverRow,
    cover_path: Option<&str>,
) -> Result<usize, CoreError> {
    Ok(conn.execute(
        "UPDATE publications_all SET cover_path = ?1
           WHERE id = ?2 AND cover_path = ?3 AND removed_at IS NULL",
        rusqlite::params![cover_path, row.id, row.rel],
    )?)
}

#[cfg(test)]
#[path = "cover_tests.rs"]
mod tests;
