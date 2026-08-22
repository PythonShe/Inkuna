//! Synthetic positions computed by the core from the canonical
//! projection (superseding the deleted shell-reporting APIs — the core
//! invents page numbers now): fixed 1024-char pages per resource, with
//! chapter ranges derived at query time via the same href-minus-fragment
//! mapping the shells use.

use super::model::ChapterPositionRange;
use crate::{CoreError, Library};

#[cfg(test)]
#[path = "positions_tests.rs"]
mod tests;

impl Library {
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
