//! The synchronous, cache-only query surface. Contract (documented per
//! method): `page` / `page_digest` / `accessibility_blocks` /
//! `page_char_range` succeed as soon as THAT page is published
//! (progressive — page 0 is available before the chapter completes);
//! `chapter`, `locate`, `hit_test`, `selection_rects`, `match_rects`,
//! `text_range`, and `word_at` require the COMPLETE chapter. Any cache
//! miss schedules the chapter at queue front and returns `NotReady` —
//! never blocks.

use std::sync::atomic::Ordering;

use crate::display::{A11yBlock, PageDisplayList, Rect};
use crate::error::EngineError;
use crate::fixed::Fx;
use crate::paginate::FxRect;
use crate::style::WritingMode;
use crate::text::Coordinate;

use super::cache::{ChapterData, SlotState};
use super::model::{ChapterGeometry, CharRange, HitResult, PageLocation, SelectionRect};
use super::session::EngineSession;

impl EngineSession {
    /// Cache lookup with the miss-schedules-and-returns-`NotReady`
    /// contract shared by every query.
    fn with_chapter<T>(
        &self,
        spine_idx: u32,
        need_complete: bool,
        f: impl FnOnce(&ChapterData, u64) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        if self.closed() {
            return Err(EngineError::NotReady);
        }
        if spine_idx >= self.spine_len() {
            return Err(EngineError::AnchorNotFound {
                detail: format!("spine index {spine_idx}"),
            });
        }
        let generation = self.shared.generation.load(Ordering::Acquire);
        let mut inner = self.shared.lock();
        inner.focus = spine_idx;
        let result = match inner.cache.get(spine_idx, generation) {
            Some(SlotState::Ready(data)) => f(data, generation),
            Some(SlotState::Laying(data)) if !need_complete => f(data, generation),
            Some(SlotState::Laying(_)) => Err(EngineError::NotReady),
            Some(SlotState::Failed(detail)) => Err(EngineError::UnsupportedContent {
                detail: detail.clone(),
            }),
            None => {
                inner.queue.push_front(spine_idx);
                Err(EngineError::NotReady)
            }
        };
        drop(inner);
        // Wake the worker: a scheduled miss, or a focus change that
        // opens new neighbor prefetch work.
        self.shared.work.notify_all();
        result
    }

    /// Whether the chapter is completely laid out at the current
    /// generation. Pure read — never schedules.
    pub fn is_ready(&self, spine_idx: u32) -> bool {
        if self.closed() {
            return false;
        }
        let generation = self.shared.generation.load(Ordering::Acquire);
        let mut inner = self.shared.lock();
        matches!(
            inner.cache.get(spine_idx, generation),
            Some(SlotState::Ready(_))
        )
    }

    /// The chapter's geometry. Requires the complete chapter.
    pub fn chapter(&self, spine_idx: u32) -> Result<ChapterGeometry, EngineError> {
        let rtl = self.shared.rtl_progression;
        self.with_chapter(spine_idx, true, |data, generation| {
            Ok(ChapterGeometry {
                generation,
                page_count: data.pages.len() as u32,
                char_range: CharRange {
                    start: 0,
                    end: data.char_len,
                },
                writing_mode: data.writing_mode,
                rtl_progression: rtl,
                truncated: data.truncated,
            })
        })
    }

    /// One page's display list. Progressive: succeeds as soon as THAT
    /// page is published.
    pub fn page(&self, spine_idx: u32, page_idx: u32) -> Result<PageDisplayList, EngineError> {
        self.with_chapter(spine_idx, false, |data, _| {
            page_of(data, page_idx).map(|(list, _)| list.clone())
        })
    }

    /// The page's canonical digest. Progressive, like [`Self::page`].
    pub fn page_digest(&self, spine_idx: u32, page_idx: u32) -> Result<String, EngineError> {
        self.with_chapter(spine_idx, false, |data, _| {
            page_of(data, page_idx).map(|(list, _)| crate::display::page_digest(list))
        })
    }

    /// The page's a11y blocks. Progressive, like [`Self::page`].
    pub fn accessibility_blocks(
        &self,
        spine_idx: u32,
        page_idx: u32,
    ) -> Result<Vec<A11yBlock>, EngineError> {
        self.with_chapter(spine_idx, false, |data, _| {
            page_of(data, page_idx).map(|(list, _)| list.a11y.clone())
        })
    }

    /// The page's canonical char range. Progressive, like [`Self::page`].
    pub fn page_char_range(
        &self,
        spine_idx: u32,
        page_idx: u32,
    ) -> Result<CharRange, EngineError> {
        self.with_chapter(spine_idx, false, |data, _| {
            page_of(data, page_idx).map(|(_, maps)| CharRange {
                start: maps.char_range.start,
                end: maps.char_range.end,
            })
        })
    }

    /// The page holding a coordinate. Offsets at or past the laid
    /// prefix clamp to the last page. Requires the complete chapter.
    pub fn locate(&self, c: Coordinate) -> Result<PageLocation, EngineError> {
        self.with_chapter(c.spine_idx, true, |data, generation| {
            if data.pages.is_empty() {
                return Err(EngineError::NotReady);
            }
            let mut page_idx = (data.pages.len() - 1) as u32;
            for (i, (_, maps)) in data.pages.iter().enumerate() {
                if c.char_offset < maps.char_range.end {
                    page_idx = i as u32;
                    break;
                }
            }
            Ok(PageLocation {
                generation,
                spine_idx: c.spine_idx,
                page_idx,
            })
        })
    }

    /// Resolves a TOC/link href to a coordinate. A fragment-free lookup
    /// resolves from the spine model alone (offset 0) and never waits;
    /// a fragment needs the chapter's anchor map, computed at layout —
    /// an un-laid chapter schedules and returns `NotReady`.
    pub fn locate_href(
        &self,
        href: &str,
        fragment: Option<&str>,
    ) -> Result<Coordinate, EngineError> {
        if self.closed() {
            return Err(EngineError::NotReady);
        }
        let normalized = inkuna_content::resolve_href("", href);
        let (path, frag_in_href) = inkuna_content::split_fragment(&normalized);
        let spine_idx = self
            .shared
            .spine
            .iter()
            .position(|h| h == path)
            .ok_or_else(|| EngineError::AnchorNotFound {
                detail: href.to_string(),
            })? as u32;
        let owned_frag = frag_in_href.map(str::to_string);
        match fragment.or(owned_frag.as_deref()) {
            None => Ok(Coordinate {
                spine_idx,
                char_offset: 0,
            }),
            Some(frag) => self.with_chapter(spine_idx, true, |data, _| {
                data.anchors
                    .iter()
                    .find(|(id, _)| id == frag)
                    .map(|&(_, char_offset)| Coordinate {
                        spine_idx,
                        char_offset,
                    })
                    .ok_or_else(|| EngineError::AnchorNotFound {
                        detail: format!("{path}#{frag}"),
                    })
            }),
        }
    }

    /// Point → coordinate (+ link target when inside a link region).
    /// Nearest-cell within nearest-band, so any point on a page
    /// resolves; an empty page resolves to `char_range.start`. Requires
    /// the complete chapter.
    pub fn hit_test(
        &self,
        spine_idx: u32,
        page_idx: u32,
        x: f64,
        y: f64,
    ) -> Result<HitResult, EngineError> {
        self.with_chapter(spine_idx, true, |data, _| {
            let (list, maps) = page_of(data, page_idx)?;
            let vertical = data.writing_mode == WritingMode::VerticalRl;
            let (fx, fy) = (Fx::from_pt(x), Fx::from_pt(y));
            let char_offset = match nearest_line(&maps.lines, vertical, fx, fy) {
                None => maps.char_range.start,
                Some(line) => {
                    let inline = if vertical { fy } else { fx };
                    nearest_cell(&line.cells, inline).unwrap_or(line.char_range.start)
                }
            };
            let link_target = list
                .links
                .iter()
                .find(|l| point_in_rect(&l.rect, x, y))
                .map(|l| l.target.clone());
            Ok(HitResult {
                coordinate: Coordinate {
                    spine_idx,
                    char_offset,
                },
                link_target,
            })
        })
    }

    /// The selection rects covering a char range: one rect per line the
    /// range touches, cells-tight on the inline axis, line-band-tall on
    /// the block axis. Requires the complete chapter.
    ///
    /// PAGE-LOCAL COORDINATES: every rect is in its own page's
    /// coordinate space, with no page attribution in the record (the
    /// FFI record shape is a frozen cross-plan contract). Callers pass
    /// ranges within a single page — obtain the page's range via
    /// [`Self::page_char_range`]. A range spanning pages returns the
    /// union of page-local rects, which is not meaningfully renderable;
    /// plan-02 shells must clamp selection ranges to the current page
    /// (cross-page drag selection is out of scope).
    pub fn selection_rects(
        &self,
        spine_idx: u32,
        range: CharRange,
    ) -> Result<Vec<SelectionRect>, EngineError> {
        self.with_chapter(spine_idx, true, |data, _| {
            let vertical = data.writing_mode == WritingMode::VerticalRl;
            let mut out = Vec::new();
            for (_, maps) in &data.pages {
                for line in &maps.lines {
                    if line.char_range.end <= range.start || line.char_range.start >= range.end {
                        continue;
                    }
                    let mut lo: Option<Fx> = None;
                    let mut hi: Option<Fx> = None;
                    for &(c, start, extent) in &line.cells {
                        if c < range.start || c >= range.end {
                            continue;
                        }
                        let end = start + extent;
                        lo = Some(lo.map_or(start, |v| v.min(start)));
                        hi = Some(hi.map_or(end, |v| v.max(end)));
                    }
                    let (Some(lo), Some(hi)) = (lo, hi) else {
                        continue;
                    };
                    let rect = if vertical {
                        FxRect {
                            x: line.band.x,
                            y: lo,
                            w: line.band.w,
                            h: hi - lo,
                        }
                    } else {
                        FxRect {
                            x: lo,
                            y: line.band.y,
                            w: hi - lo,
                            h: line.band.h,
                        }
                    };
                    out.push(SelectionRect {
                        rect: to_rect(rect),
                        writing_mode: data.writing_mode,
                    });
                }
            }
            Ok(out)
        })
    }

    /// The rects of one search match — exactly
    /// `selection_rects(spine, off..off+len)`, including its PAGE-LOCAL
    /// coordinate contract: rects carry no page attribution, so callers
    /// clamp the match range to one page (via
    /// [`Self::page_char_range`]) before rendering.
    pub fn match_rects(
        &self,
        spine_idx: u32,
        char_offset: u64,
        len: u64,
    ) -> Result<Vec<SelectionRect>, EngineError> {
        self.selection_rects(
            spine_idx,
            CharRange {
                start: char_offset,
                end: char_offset.saturating_add(len),
            },
        )
    }

    /// The word containing a coordinate, per the worker's ICU word
    /// segmenter (CJK-aware). Answered from the chapter's cached
    /// [`TextIndex`](super::cache::TextIndex) — the word boundaries are
    /// computed once, at layout publish time, never per call. Requires
    /// the complete chapter.
    pub fn word_at(&self, c: Coordinate) -> Result<CharRange, EngineError> {
        self.with_chapter(c.spine_idx, true, |data, _| {
            let total = data.index.chars;
            if total == 0 {
                return Ok(CharRange { start: 0, end: 0 });
            }
            let off = c.char_offset.min(total - 1);
            // `word_bounds` starts at 0 and ends at `total`, so the
            // first edge past `off` always has a predecessor.
            let bounds = &data.index.word_bounds;
            let i = bounds.partition_point(|&e| e <= off);
            let start = if i == 0 { 0 } else { bounds[i - 1] };
            let end = bounds.get(i).copied().unwrap_or(total);
            Ok(CharRange { start, end })
        })
    }

    /// A char range as text, sliced from the canonical projection.
    /// Char→byte resolution goes through the chapter's cached stride
    /// table (bounded scan, no per-call allocation). Requires the
    /// complete chapter.
    pub fn text_range(&self, spine_idx: u32, range: CharRange) -> Result<String, EngineError> {
        self.with_chapter(spine_idx, true, |data, _| {
            let total = data.index.chars;
            let start = range.start.min(total);
            let end = range.end.clamp(start, total);
            Ok(data.text[data.byte_of_char(start)..data.byte_of_char(end)].to_string())
        })
    }
}

/// The page at `page_idx`, distinguishing not-yet-published
/// (`NotReady`) from past-the-end on a complete chapter
/// (`AnchorNotFound`).
fn page_of(
    data: &ChapterData,
    page_idx: u32,
) -> Result<&(PageDisplayList, crate::display::PageMaps), EngineError> {
    match data.pages.get(page_idx as usize) {
        Some(page) => Ok(page),
        None if data.complete => Err(EngineError::AnchorNotFound {
            detail: format!("page {page_idx}"),
        }),
        None => Err(EngineError::NotReady),
    }
}

/// The line whose band is nearest the point on the block axis
/// (contained → distance 0; ties → first).
fn nearest_line<'a>(
    lines: &'a [crate::display::LineGeom],
    vertical: bool,
    x: Fx,
    y: Fx,
) -> Option<&'a crate::display::LineGeom> {
    let mut best: Option<(i64, &crate::display::LineGeom)> = None;
    for line in lines {
        let (lo, hi, p) = if vertical {
            (line.band.x.0, line.band.x.saturating_add(line.band.w).0, x.0)
        } else {
            (line.band.y.0, line.band.y.saturating_add(line.band.h).0, y.0)
        };
        let d = if p < lo {
            i64::from(lo) - i64::from(p)
        } else if p > hi {
            i64::from(p) - i64::from(hi)
        } else {
            0
        };
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, line));
        }
    }
    best.map(|(_, line)| line)
}

/// The char of the cell containing (or nearest to) the inline
/// coordinate; ties → first.
fn nearest_cell(cells: &[(u64, Fx, Fx)], inline: Fx) -> Option<u64> {
    let p = i64::from(inline.0);
    let mut best: Option<(i64, u64)> = None;
    for &(c, start, extent) in cells {
        let lo = i64::from(start.0);
        let hi = lo + i64::from(extent.0.max(0));
        let d = if p < lo {
            lo - p
        } else if p >= hi {
            p - hi + 1
        } else {
            0
        };
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, c));
        }
        if d == 0 {
            break;
        }
    }
    best.map(|(_, c)| c)
}

fn point_in_rect(r: &Rect, x: f64, y: f64) -> bool {
    x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height
}

fn to_rect(r: FxRect) -> Rect {
    Rect {
        x: f64::from(r.x.to_f32()),
        y: f64::from(r.y.to_f32()),
        width: f64::from(r.w.to_f32()),
        height: f64::from(r.h.to_f32()),
    }
}
