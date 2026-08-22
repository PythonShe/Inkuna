//! The reader records mirrored 1:1 from `inkuna-core`'s engine
//! re-exports, with `From` conversions beside each.

/// A content coordinate: THE reading position. `char_offset` counts
/// Unicode scalars into the resource's canonical projection, so the
/// same coordinate means the same character on every platform and
/// under every layout setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct Coordinate {
    /// Index into the spine, in reading order.
    pub spine_idx: u32,
    /// Unicode scalar count into the resource's canonical projection.
    pub char_offset: u64,
}

impl From<inkuna_core::Coordinate> for Coordinate {
    fn from(c: inkuna_core::Coordinate) -> Self {
        Coordinate {
            spine_idx: c.spine_idx,
            char_offset: c.char_offset,
        }
    }
}

impl From<Coordinate> for inkuna_core::Coordinate {
    fn from(c: Coordinate) -> Self {
        inkuna_core::Coordinate {
            spine_idx: c.spine_idx,
            char_offset: c.char_offset,
        }
    }
}
