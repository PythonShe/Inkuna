//! Per-resource position ranges: the shell reports them once Readium has
//! computed synthetic positions, and chapter ranges are derived from them
//! at query time via the same href-minus-fragment mapping the shells use.

use super::model::ChapterPositionRange;
use crate::{CoreError, Library};

#[cfg(test)]
#[path = "positions_tests.rs"]
mod tests;

impl Library {
    /// Records the navigator's per-resource position counts, one entry per
    /// reading-order (spine) resource, replacing any previous report. The
    /// core derives cumulative 1-based `start_position`s and keeps
    /// `publications.position_count` in agreement with the total in the
    /// same transaction, so this supersedes
    /// [`report_position_count`](Self::report_position_count) when the
    /// shell has the full breakdown. An empty report clears the ranges and
    /// leaves `position_count` untouched — no data beats wrong data.
    pub fn report_position_ranges(&self, id: &str, counts: &[u32]) -> Result<(), CoreError> {
        let mut conn = self.writer.lock().unwrap();
        let tx = conn.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM publications WHERE id = ?1)",
            [id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(CoreError::NotFound(id.to_string()));
        }

        tx.execute("DELETE FROM resource_positions WHERE publication_id = ?1", [id])?;
        let mut start: i64 = 1;
        {
            let mut insert = tx.prepare_cached(
                "INSERT INTO resource_positions
                    (publication_id, spine_idx, start_position, position_count)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (spine_idx, &count) in counts.iter().enumerate() {
                insert.execute(rusqlite::params![id, spine_idx as i64, start, count])?;
                start += i64::from(count);
            }
        }
        if !counts.is_empty() {
            let total = start - 1;
            tx.execute(
                "UPDATE publications SET position_count = ?1 WHERE id = ?2",
                rusqlite::params![total, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Every TOC entry's position span, in chapter order. Empty until the
    /// shell has reported ranges (or when the book has no TOC); sparse for
    /// chapters whose href matches no spine resource. A chapter's span runs
    /// from its own resource's first position to the position before the
    /// next chapter's resource — so a chapter covering several spine items
    /// spans all of them, and fragment-anchored chapters sharing one
    /// resource each report that whole resource.
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
        for (i, &(chapter_idx, spine_idx)) in placed.iter().enumerate() {
            let Some(&(_, start, count)) = range_for(spine_idx) else {
                continue;
            };
            // First following chapter on a *different* resource bounds this
            // one; fragment siblings on the same resource share its span.
            let end = placed[i + 1..]
                .iter()
                .find(|&&(_, next)| next != spine_idx)
                .and_then(|&(_, next)| range_for(next))
                .map(|&(_, next_start, _)| next_start.saturating_sub(1))
                .unwrap_or(last);
            // A TOC ordered against the spine (or a resource with zero
            // positions) degrades to the chapter's own resource span.
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
