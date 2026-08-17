//! The progress domain's derived types.

/// One TOC entry's span of Readium synthetic positions, derived from the
/// shell-reported per-resource ranges. Both bounds are 1-based and
/// inclusive; a chapter that shares its resource with fragment-anchored
/// siblings reports the whole resource span for each of them — positions
/// are resource-granular and cannot split inside one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterPositionRange {
    /// `Chapter::idx` of the TOC entry this range belongs to. Sparse:
    /// chapters whose href matches no spine resource are absent.
    pub chapter_idx: u32,
    pub start_position: u32,
    pub end_position: u32,
}
