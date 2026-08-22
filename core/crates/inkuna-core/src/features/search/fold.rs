//! Match-time normalization: a per-char NFKC + full case fold applied
//! identically to corpus and query, with an offset map back to the
//! original text so hits carry original char offsets and snippets show
//! the author's text, not the folded form.
//!
//! The fold is per-char on purpose: string-level NFKC can merge or
//! reorder across char boundaries, which would make offsets ambiguous.
//! Per-char NFKC still captures what matters for search — full-width
//! alphanumerics (ＡＢＣ→abc, common in CJK books), compatibility
//! singletons, ligatures — and both sides of every comparison go through
//! the same transform, so matching stays internally consistent.
//!
//! INVARIANT: no folded offset ever crosses the FFI. Every hit
//! [`FoldedText::occurrences`] yields (and hence `BookSearchHit`'s
//! `char_offset` and its `progression` denominator) indexes the ORIGINAL
//! `resource_text` body — the canonical projection — in Unicode scalars.
//! With the corpus being exactly the projection, a hit IS a content
//! coordinate: it feeds `locate` / `match_rects` with no conversion
//! step. A fold expansion (`ﬁ`→"fi") or contraction (`㍿`→株式会社)
//! shifts folded-space offsets, never the offsets returned here.

/// How many chars of context a snippet keeps around a match; shared with
/// the shells' result rows (2-line snippet labels).
pub(super) const SNIPPET_BEFORE: usize = 34;
pub(super) const SNIPPET_AFTER: usize = 46;

/// A folded haystack plus the map back into the original text.
pub(super) struct FoldedText {
    pub(super) folded: String,
    /// One entry per folded char: its byte offset in `folded` and the
    /// original char index it came from. Sorted by construction.
    map: Vec<(u32, u32)>,
    /// Byte offset of every original char, for O(log n) snippet slicing.
    orig_byte_of_char: Vec<u32>,
    orig_char_count: u32,
}

fn for_each_folded(c: char, mut push: impl FnMut(char)) {
    if c.is_ascii() {
        push(c.to_ascii_lowercase());
        return;
    }
    // Both constructors hand back borrows of compiled data — free to call.
    let nfkc = icu_normalizer::ComposingNormalizer::new_nfkc();
    let mapper = icu_casemap::CaseMapper::new();
    let mut buf = [0u8; 4];
    for n in nfkc.normalize(c.encode_utf8(&mut buf)).chars() {
        let mut nbuf = [0u8; 4];
        for f in mapper.fold_string(n.encode_utf8(&mut nbuf)).chars() {
            push(f);
        }
    }
}

/// Folds a query with the exact per-char pipeline the corpus gets.
pub(super) fn fold_query(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for c in query.chars() {
        for_each_folded(c, |f| out.push(f));
    }
    out
}

pub(super) fn fold_text(text: &str) -> FoldedText {
    let mut folded = String::with_capacity(text.len());
    let mut map = Vec::with_capacity(text.len() / 2);
    let mut orig_byte_of_char = Vec::with_capacity(text.len() / 2);
    for (orig_idx, (byte_idx, c)) in text.char_indices().enumerate() {
        orig_byte_of_char.push(byte_idx as u32);
        for_each_folded(c, |f| {
            map.push((folded.len() as u32, orig_idx as u32));
            folded.push(f);
        });
    }
    FoldedText {
        folded,
        map,
        orig_char_count: orig_byte_of_char.len() as u32,
        orig_byte_of_char,
    }
}

impl FoldedText {
    /// Original char index of the folded char at `folded_byte`.
    fn orig_char_at(&self, folded_byte: usize) -> u32 {
        let i = self
            .map
            .partition_point(|&(b, _)| (b as usize) < folded_byte);
        self.map
            .get(i)
            .map(|&(_, orig)| orig)
            .unwrap_or(self.orig_char_count)
    }

    /// Every occurrence of the folded needle, including overlapping matches,
    /// as original-text char ranges `[start, end)`.
    pub(super) fn occurrences<'a>(
        &'a self,
        needle: &'a str,
    ) -> impl Iterator<Item = (u32, u32)> + 'a {
        self.folded.char_indices().filter_map(move |(byte, _)| {
            self.folded.get(byte..)?.starts_with(needle).then(|| {
                let start = self.orig_char_at(byte);
                let end = self.orig_char_at(byte + needle.len());
                // A match ending inside one original char's fold expansion
                // (the first `s` of `ß` → `ss`) still covers that char.
                (start, end.max(start + 1))
            })
        })
    }
}

/// The three snippet pieces around `[start, end)` (original char
/// indices), with `…` marking a truncated side. `text` must be the
/// original text `folded` was built from.
pub(super) fn snippet(
    text: &str,
    folded: &FoldedText,
    start: u32,
    end: u32,
) -> (String, String, String) {
    let byte_of = |char_idx: u32| -> usize {
        folded
            .orig_byte_of_char
            .get(char_idx as usize)
            .map(|&b| b as usize)
            .unwrap_or(text.len())
    };
    let pre_from = start.saturating_sub(SNIPPET_BEFORE as u32);
    let post_to = (end + SNIPPET_AFTER as u32).min(folded.orig_char_count);

    let mut pre = String::new();
    if pre_from > 0 {
        pre.push('…');
    }
    pre.push_str(text[byte_of(pre_from)..byte_of(start)].trim_start());
    let matched = text[byte_of(start)..byte_of(end)].to_string();
    let mut post = text[byte_of(end)..byte_of(post_to)].trim_end().to_string();
    if post_to < folded.orig_char_count {
        post.push('…');
    }
    (pre, matched, post)
}
