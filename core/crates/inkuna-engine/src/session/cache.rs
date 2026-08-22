//! The chapter cache: LRU over complete chapters, keyed
//! `(spine_idx, generation)`. The current (focused) chapter is never
//! evicted; in-progress slots are never evicted (the worker is writing
//! into them).

use crate::display::{PageDisplayList, PageMaps};
use crate::style::WritingMode;

/// Complete chapters retained beyond the current one.
pub(super) const CACHE_CAPACITY: usize = 5;

/// Sampling stride (in chars) of [`TextIndex::stride_bytes`]: char→byte
/// resolution costs at most one `CHAR_STRIDE`-char scan — bounded and
/// allocation-free, so queries never do O(chapter) work under the
/// session mutex after the index is published.
pub(super) const CHAR_STRIDE: usize = 1024;

/// Derived text indexes, computed ONCE by the worker (off the session
/// mutex) when the chapter's projection publishes: `word_at` and
/// `text_range` answer from these instead of re-deriving per call.
#[derive(Default)]
pub(super) struct TextIndex {
    /// Total chars in the projection text.
    pub chars: u64,
    /// ICU word-segment edges as ascending char offsets, first `0`,
    /// last `chars` (just `[0]` for empty text).
    pub word_bounds: Vec<u64>,
    /// Byte offset of every `CHAR_STRIDE`-th char (`[0]` minimum).
    pub stride_bytes: Vec<u32>,
}

impl TextIndex {
    /// Builds the index in one pass over `text` plus one segmenter run.
    /// Worker-side only — never call under the session mutex.
    pub fn build(
        segmenter: &icu_segmenter::WordSegmenterBorrowed<'static>,
        text: &str,
    ) -> TextIndex {
        let mut stride_bytes = vec![0u32];
        let mut chars = 0u64;
        for (i, (b, _)) in text.char_indices().enumerate() {
            if i > 0 && i % CHAR_STRIDE == 0 {
                stride_bytes.push(b as u32);
            }
            chars = (i + 1) as u64;
        }
        // Edges are ascending byte offsets on char boundaries, so one
        // tandem walk converts them to char offsets without a per-char
        // table.
        let mut word_bounds = vec![0u64];
        let mut count = 0u64;
        let mut it = text.char_indices().peekable();
        for edge in segmenter.segment_str(text) {
            if edge == 0 {
                continue;
            }
            while it.next_if(|&(b, _)| b < edge).is_some() {
                count += 1;
            }
            if word_bounds.last() != Some(&count) {
                word_bounds.push(count);
            }
        }
        if chars > 0 && word_bounds.last() != Some(&chars) {
            word_bounds.push(chars);
        }
        TextIndex {
            chars,
            word_bounds,
            stride_bytes,
        }
    }
}

/// One chapter's published layout, filled progressively: `text`,
/// `anchors`, `writing_mode`, and the text `index` land when projection
/// completes, pages as they are emitted, and `complete` + final counts
/// at the end.
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
    /// Worker-computed text indexes; see [`TextIndex`].
    pub index: TextIndex,
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
            index: TextIndex::default(),
        }
    }

    /// Byte offset of char offset `c` (clamped to the text end), via
    /// the stride table + a bounded ≤[`CHAR_STRIDE`]-char scan. No
    /// allocation, so it is safe under the session mutex.
    pub fn byte_of_char(&self, c: u64) -> usize {
        if c >= self.index.chars {
            return self.text.len();
        }
        let c = c as usize;
        let slot = (c / CHAR_STRIDE).min(self.index.stride_bytes.len().saturating_sub(1));
        let base = self.index.stride_bytes[slot] as usize;
        let rem = c - slot * CHAR_STRIDE;
        match self.text[base..].char_indices().nth(rem) {
            Some((b, _)) => base + b,
            None => self.text.len(),
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
