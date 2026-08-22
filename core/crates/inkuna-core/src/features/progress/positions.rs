//! Synthetic positions computed by the core from the canonical
//! projection (superseding the deleted shell-reporting APIs — the core
//! invents page numbers now): fixed 1024-char pages per resource, with
//! chapter ranges derived at query time via the same href-minus-fragment
//! mapping the shells use.

use inkuna_engine::Coordinate;

use super::model::ChapterPositionRange;
use crate::{CoreError, Library};

#[cfg(test)]
#[path = "positions_tests.rs"]
mod tests;

/// Chars of canonical projection per synthetic position — the one place
/// the constant lives; import, the reconcile pass, and position lookups
/// all divide by it here.
pub(crate) const CHARS_PER_POSITION: u64 = 1024;

/// Per-resource synthetic position ranges from per-resource projected
/// char counts: `ceil(char_len / 1024).max(1)` positions each — a
/// textless resource still occupies one position, so position math never
/// has spine holes — with cumulative 1-based starts. Returns
/// `(spine_idx, start_position, position_count)` rows, exactly the shape
/// `resource_positions` stores. The core invents these page numbers now
/// (superseding the deleted shell-reporting APIs): they are a pure
/// function of the canonical projection, identical on every platform.
pub(crate) fn synthetic_positions(char_counts: &[u64]) -> Vec<(u32, u32, u32)> {
    let mut start: u32 = 1;
    char_counts
        .iter()
        .enumerate()
        .map(|(spine_idx, &chars)| {
            let count = chars.div_ceil(CHARS_PER_POSITION).max(1) as u32;
            let row = (spine_idx as u32, start, count);
            start = start.saturating_add(count);
            row
        })
        .collect()
}

/// A coordinate's 1-based synthetic position within known ranges
/// (`(spine_idx, start_position, position_count)` rows, spine order) —
/// `start_position + char_offset / 1024`, clamped into the resource's
/// range; a coordinate past every known resource clamps to the last
/// position. `None` only when `ranges` is empty. THE `/1024` derivation:
/// `position_of`, the FFI reader session's snapshot lookup, and
/// `update_progress`'s internal derivation all come through here — the
/// shells never mirror the 1024-char constant.
pub fn position_for(ranges: &[(u32, u32, u32)], coordinate: Coordinate) -> Option<u32> {
    let &(last_spine, last_start, last_count) = ranges.last()?;
    match ranges
        .iter()
        .find(|&&(spine_idx, _, _)| spine_idx == coordinate.spine_idx)
    {
        Some(&(_, start, count)) => {
            let within = (coordinate.char_offset / CHARS_PER_POSITION)
                .min(u64::from(count.saturating_sub(1))) as u32;
            Some(start.saturating_add(within))
        }
        None if coordinate.spine_idx > last_spine => {
            Some(last_start.saturating_add(last_count.saturating_sub(1)))
        }
        // A hole below the last known resource cannot arise from the
        // writers (they cover every spine index); fall back to the start.
        None => Some(1),
    }
}

/// One publication's `resource_positions` rows in spine order, as
/// `(spine_idx, start_position, position_count)`.
pub(crate) fn position_ranges_on(
    conn: &rusqlite::Connection,
    id: &str,
) -> Result<Vec<(u32, u32, u32)>, rusqlite::Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT spine_idx, start_position, position_count FROM resource_positions
         WHERE publication_id = ?1 ORDER BY spine_idx",
    )?;
    let rows = stmt.query_map([id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
    rows.collect()
}

impl Library {
    /// The 1-based synthetic position of `coordinate` — session-free, so
    /// Home/Detail screens can label "page N of M" without a reader
    /// session. A book with no position rows answers `1`; past-end
    /// coordinates clamp to the last position. `NotFound` when the book
    /// does not exist.
    pub fn position_of(&self, id: &str, coordinate: Coordinate) -> Result<u32, CoreError> {
        let ranges = self.position_ranges(id)?;
        match position_for(&ranges, coordinate) {
            Some(position) => Ok(position),
            None => {
                // Distinguish "no rows yet" from "no such publication".
                self.publication(id)?;
                Ok(1)
            }
        }
    }

    /// The publication's total synthetic position count, session-free.
    /// A book with no position rows answers `1`. `NotFound` when the
    /// book does not exist.
    pub fn position_count(&self, id: &str) -> Result<u32, CoreError> {
        let ranges = self.position_ranges(id)?;
        match ranges.last() {
            Some(&(_, start, count)) => Ok(start.saturating_add(count.saturating_sub(1))),
            None => {
                self.publication(id)?;
                Ok(1)
            }
        }
    }

    /// The publication's `resource_positions` ranges in spine order, as
    /// `(spine_idx, start_position, position_count)` — the snapshot the
    /// FFI reader session loads once at open so its position lookups
    /// stay sync-safe with no DB access afterwards. Empty when the book
    /// has no rows yet (never `NotFound` on its own).
    pub fn position_ranges(&self, id: &str) -> Result<Vec<(u32, u32, u32)>, CoreError> {
        self.readers
            .with(|conn| position_ranges_on(conn, id).map_err(Into::into))
    }
    /// Every TOC entry's position span, in chapter order. Empty until the
    /// book's synthetic positions are computed (or when the book has no
    /// TOC); sparse for chapters whose href matches no spine resource. A
    /// chapter's span runs from its own resource's first position to the
    /// position before the next chapter's resource — so a chapter covering
    /// several spine items spans all of them, and fragment-anchored
    /// chapters sharing one resource each report that whole resource.
    pub fn chapter_position_ranges(
        &self,
        id: &str,
    ) -> Result<Vec<ChapterPositionRange>, CoreError> {
        let (chapters, resources, ranges) = self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT idx, href FROM chapters WHERE publication_id = ?1 ORDER BY idx",
            )?;
            let chapters: Vec<(u32, String)> = stmt
                .query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?;

            let mut stmt = conn.prepare_cached(
                "SELECT spine_idx, href FROM resources
                 WHERE publication_id = ?1 ORDER BY spine_idx",
            )?;
            let resources: Vec<(u32, String)> = stmt
                .query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?;

            let mut stmt = conn.prepare_cached(
                "SELECT spine_idx, start_position, position_count FROM resource_positions
                 WHERE publication_id = ?1 ORDER BY spine_idx",
            )?;
            let ranges: Vec<(u32, u32, u32)> = stmt
                .query_map([id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect::<Result<_, _>>()?;

            Ok((chapters, resources, ranges))
        })?;

        if ranges.is_empty() || chapters.is_empty() {
            // Distinguish "no data yet" from "no such publication".
            self.publication(id)?;
            return Ok(Vec::new());
        }

        let range_for = |spine_idx: u32| ranges.iter().find(|r| r.0 == spine_idx);
        let last = ranges
            .last()
            .map(|&(_, start, count)| start + count.saturating_sub(1))
            .unwrap_or(1);

        // Each chapter's spine resource, via href minus fragment.
        let resource_of = |href: &str| {
            let base = href.split('#').next().unwrap_or(href);
            resources
                .iter()
                .find(|(_, r)| r.split('#').next().unwrap_or(r) == base)
                .map(|&(idx, _)| idx)
        };
        let placed: Vec<(u32, u32)> = chapters
            .iter()
            .filter_map(|(idx, href)| resource_of(href).map(|r| (*idx, r)))
            .collect();

        let mut out = Vec::with_capacity(placed.len());
        for &(chapter_idx, spine_idx) in &placed {
            let Some(&(_, start, count)) = range_for(spine_idx) else {
                continue;
            };
            // Chapter navigation order is independent of the reading order.
            // Bound a chapter by the next TOC resource in spine order, not by
            // the next entry in the TOC vector.
            let end = placed
                .iter()
                .map(|&(_, resource)| resource)
                .filter(|&resource| resource > spine_idx)
                .min()
                .and_then(range_for)
                .map(|&(_, next_start, _)| next_start.saturating_sub(1))
                .unwrap_or(last);
            let own_end = start + count.saturating_sub(1);
            out.push(ChapterPositionRange {
                chapter_idx,
                start_position: start,
                end_position: end.max(own_end).max(start),
            });
        }
        Ok(out)
    }
}
