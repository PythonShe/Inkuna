//! Cover normalization: covers are downscaled and re-encoded to lossy
//! WebP at import, so `covers/` only ever holds display-sized files and
//! the shells can decode any cover without pathological allocations. A
//! cover that cannot be decoded (SVG, exotic codecs, corrupt data) passes
//! through untouched — a cover is optional data and never fails an
//! import.

use std::io::Cursor;
use std::sync::Mutex;

use image::imageops::FilterType;
use image::ImageReader;

use crate::formats::epub::Cover;
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
    let _decode_slot = DECODE_GATE.lock().unwrap();
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

impl Library {
    /// Re-encodes covers persisted before normalization existed (or by
    /// older versions) into the bounded WebP form, returning how many
    /// changed. Idempotent — already-normalized covers are skipped — and
    /// per-cover failures are logged and skipped rather than failing the
    /// pass: a cover is derived data. Safe to run in the background at
    /// any time after open.
    pub fn optimize_covers(&self) -> Result<u32, CoreError> {
        let rows: Vec<(String, String)> = self.readers.with(|conn| {
            let mut stmt = conn
                .prepare("SELECT id, cover_path FROM publications WHERE cover_path IS NOT NULL")?;
            let rows = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })?;

        // Sequential on purpose: the decode dominates and [`DECODE_GATE`]
        // serializes it anyway, so fanning out on rayon would only pin
        // worker threads against the gate during app startup.
        let mut changed = 0;
        for (id, rel) in &rows {
            match self.optimize_cover(id, rel) {
                Ok(true) => changed += 1,
                Ok(false) => {}
                Err(error) => log::warn!("cover optimization skipped for {id}: {error}"),
            }
        }
        Ok(changed)
    }

    /// Normalizes one stored cover in place: write the WebP beside the
    /// old file (tmp + rename, so a same-path rewrite can never leave a
    /// torn file), point the row at it, then delete the old file. A crash
    /// between the steps leaves an unreferenced file for the open-time
    /// sweep, never a row without its cover.
    fn optimize_cover(&self, id: &str, rel: &str) -> Result<bool, CoreError> {
        let extension = rel.rsplit_once('.').map(|(_, s)| s).unwrap_or("");
        let bytes = std::fs::read(self.data_dir.join(rel))?;
        let Some(cover) = normalized(&bytes, extension) else {
            return Ok(false);
        };

        let new_rel = format!("covers/{id}.{}", cover.extension);
        let new_path = self.data_dir.join(&new_rel);
        let tmp_path = self
            .data_dir
            .join(format!("covers/{id}.{}.tmp", cover.extension));
        std::fs::write(&tmp_path, &cover.bytes).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp_path);
        })?;
        std::fs::rename(&tmp_path, &new_path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp_path);
        })?;

        let updated = {
            let conn = self.writer.lock().unwrap();
            conn.execute(
                "UPDATE publications SET cover_path = ?1 WHERE id = ?2 AND cover_path = ?3",
                rusqlite::params![new_rel, id, rel],
            )?
        };
        if updated == 0 {
            // The row moved or vanished under us; withdraw the new file
            // unless it landed on the old path itself.
            if new_rel != rel {
                let _ = std::fs::remove_file(&new_path);
            }
            return Ok(false);
        }
        if new_rel != rel {
            let _ = std::fs::remove_file(self.data_dir.join(rel));
        }
        Ok(true)
    }
}

#[cfg(test)]
#[path = "cover_tests.rs"]
mod tests;
