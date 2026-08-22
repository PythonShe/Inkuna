//! The chapter cache: LRU over complete chapters, keyed
//! `(spine_idx, generation)`. The current (focused) chapter is never
//! evicted; in-progress slots are never evicted (the worker is writing
//! into them).

use crate::display::{PageDisplayList, PageMaps};
use crate::style::WritingMode;

/// Complete chapters retained beyond the current one.
pub(super) const CACHE_CAPACITY: usize = 5;

/// One chapter's published layout, filled progressively: `text`,
/// `anchors`, and `writing_mode` land when projection completes, pages
/// as they are emitted, and `complete` + final counts at the end.
pub(super) struct ChapterData {
    pub pages: Vec<(PageDisplayList, PageMaps)>,
    /// The canonical projection text (full, even when layout truncated).
    pub text: String,
    /// Chars covered by the emitted pages — the laid prefix.
    pub char_len: u64,
    pub anchors: Vec<(String, u64)>,
    pub writing_mode: WritingMode,
    pub truncated: bool,
    pub complete: bool,
}

impl ChapterData {
    pub fn empty() -> ChapterData {
        ChapterData {
            pages: Vec::new(),
            text: String::new(),
            char_len: 0,
            anchors: Vec::new(),
            writing_mode: WritingMode::HorizontalTb,
            truncated: false,
            complete: false,
        }
    }
}

pub(super) enum SlotState {
    /// The worker is laying this chapter out; pages arrive one by one.
    Laying(ChapterData),
    /// Complete.
    Ready(ChapterData),
    /// The resource failed closed (parse failure, unreadable entry, a
    /// layout panic). Scoped to the chapter; the book stays usable.
    Failed(String),
}

struct Slot {
    spine_idx: u32,
    generation: u64,
    state: SlotState,
    used: u64,
}

#[derive(Default)]
pub(super) struct Cache {
    slots: Vec<Slot>,
    tick: u64,
}

impl Cache {
    /// Looks a chapter up, marking it most-recently-used.
    pub fn get(&mut self, spine_idx: u32, generation: u64) -> Option<&SlotState> {
        self.tick += 1;
        let tick = self.tick;
        let slot = self
            .slots
            .iter_mut()
            .find(|s| s.spine_idx == spine_idx && s.generation == generation)?;
        slot.used = tick;
        Some(&slot.state)
    }

    /// The slot's state for the worker to write into; no LRU touch.
    pub fn state_mut(&mut self, spine_idx: u32, generation: u64) -> Option<&mut SlotState> {
        self.slots
            .iter_mut()
            .find(|s| s.spine_idx == spine_idx && s.generation == generation)
            .map(|s| &mut s.state)
    }

    pub fn contains(&self, spine_idx: u32, generation: u64) -> bool {
        self.slots
            .iter()
            .any(|s| s.spine_idx == spine_idx && s.generation == generation)
    }

    /// Claims a fresh in-progress slot.
    pub fn insert_laying(&mut self, spine_idx: u32, generation: u64) {
        self.tick += 1;
        self.slots.push(Slot {
            spine_idx,
            generation,
            state: SlotState::Laying(ChapterData::empty()),
            used: self.tick,
        });
    }

    /// Evicts least-recently-used complete slots beyond
    /// [`CACHE_CAPACITY`]. `current` and in-progress slots survive.
    pub fn evict(&mut self, current: u32) {
        loop {
            let done = self
                .slots
                .iter()
                .filter(|s| !matches!(s.state, SlotState::Laying(_)))
                .count();
            if done <= CACHE_CAPACITY {
                return;
            }
            let victim = self
                .slots
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    s.spine_idx != current && !matches!(s.state, SlotState::Laying(_))
                })
                .min_by_key(|(_, s)| s.used)
                .map(|(i, _)| i);
            match victim {
                Some(i) => {
                    self.slots.remove(i);
                }
                None => return,
            }
        }
    }

    /// Drops everything — the `update_layout` invalidation.
    pub fn clear(&mut self) {
        self.slots.clear();
    }
}
