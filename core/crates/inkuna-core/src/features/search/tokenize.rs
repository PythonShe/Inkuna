//! The index's two tokenizers.
//!
//! `WordTokenizer` ("ink_word") feeds the scoring field: jieba
//! search-mode segmentation for CJK runs (so 月光書房 indexes 月光 and
//! 書房, and multi-word Chinese queries rank on real words), folded
//! alphanumeric words for everything else.
//!
//! `CjkUnigramTokenizer` ("ink_uni") feeds the exact-match field: every
//! Han / kana / Hangul char is its own positioned token, which makes a
//! single-character query a term lookup and an exact CJK substring a
//! phrase query — the two shapes jieba's word tokens cannot answer.
//! Non-CJK runs emit no token but do advance the position counter, so a
//! phrase never matches across intervening Latin text or punctuation.

use std::sync::LazyLock;

use jieba_rs::Jieba;
use tantivy::tokenizer::{Token, TokenStream, Tokenizer};

use super::fold::fold_query;

/// Loaded once per process; the default dictionary parse is the expensive
/// part and every import and library-wide search shares it.
static JIEBA: LazyLock<Jieba> = LazyLock::new(Jieba::new);

/// The scripts whose single characters are complete search terms.
pub(super) fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{2E80}'..='\u{2EFF}'   // CJK radicals
        | '\u{3040}'..='\u{30FF}' // hiragana, katakana
        | '\u{3130}'..='\u{318F}' // Hangul compatibility jamo
        | '\u{31F0}'..='\u{31FF}' // katakana phonetic extensions
        | '\u{3400}'..='\u{4DBF}' // CJK extension A
        | '\u{4E00}'..='\u{9FFF}' // CJK unified ideographs
        | '\u{A960}'..='\u{A97F}' // Hangul jamo extended-A
        | '\u{AC00}'..='\u{D7FF}' // Hangul syllables + jamo extended-B
        | '\u{F900}'..='\u{FAFF}' // CJK compatibility ideographs
        | '\u{FF66}'..='\u{FF9D}' // half-width katakana
        | '\u{20000}'..='\u{2FA1F}' // CJK extensions B..F + supplement
        | '\u{30000}'..='\u{323AF}' // CJK extensions G + H
    )
}

/// A materialized token list; both tokenizers analyze eagerly and stream
/// from the vec, which keeps the `Tokenizer` impls trivially cloneable.
#[derive(Default)]
pub(super) struct VecTokenStream {
    tokens: Vec<Token>,
    cursor: Option<usize>,
}

impl TokenStream for VecTokenStream {
    fn advance(&mut self) -> bool {
        let next = self.cursor.map_or(0, |c| c + 1);
        self.cursor = Some(next);
        next < self.tokens.len()
    }

    fn token(&self) -> &Token {
        &self.tokens[self.cursor.unwrap_or(0)]
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.cursor.unwrap_or(0)]
    }
}

/// One maximal run of same-kind chars: CJK, word (alphanumeric), or other.
fn runs(text: &str) -> impl Iterator<Item = (usize, &str, RunKind)> {
    let mut runs = Vec::new();
    let mut start = 0;
    let mut kind: Option<RunKind> = None;
    for (idx, c) in text.char_indices() {
        let k = RunKind::of(c);
        if kind != Some(k) {
            if let Some(prev) = kind {
                runs.push((start, &text[start..idx], prev));
            }
            start = idx;
            kind = Some(k);
        }
    }
    if let Some(prev) = kind {
        runs.push((start, &text[start..], prev));
    }
    runs.into_iter()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RunKind {
    Cjk,
    Word,
    Other,
}

impl RunKind {
    fn of(c: char) -> RunKind {
        if is_cjk(c) {
            RunKind::Cjk
        } else if c.is_alphanumeric() {
            RunKind::Word
        } else {
            RunKind::Other
        }
    }
}

fn push(tokens: &mut Vec<Token>, position: &mut usize, offset: usize, len: usize, text: String) {
    tokens.push(Token {
        offset_from: offset,
        offset_to: offset + len,
        position: *position,
        text,
        position_length: 1,
    });
    *position += 1;
}

/// jieba words for CJK runs, folded words elsewhere. Also the analyzer
/// for word *queries*: [`word_terms`] reuses it so query and index agree.
#[derive(Clone, Default)]
pub(super) struct WordTokenizer;

impl Tokenizer for WordTokenizer {
    type TokenStream<'a> = VecTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> VecTokenStream {
        let mut tokens = Vec::new();
        let mut position = 0;
        for (offset, run, kind) in runs(text) {
            match kind {
                RunKind::Cjk => {
                    for token in JIEBA.cut_for_search(run, true) {
                        push(
                            &mut tokens,
                            &mut position,
                            offset + token.byte_start,
                            token.byte_end - token.byte_start,
                            token.word.to_string(),
                        );
                    }
                }
                RunKind::Word => {
                    push(
                        &mut tokens,
                        &mut position,
                        offset,
                        run.len(),
                        fold_query(run),
                    );
                }
                RunKind::Other => {}
            }
        }
        VecTokenStream {
            tokens,
            cursor: None,
        }
    }
}

/// One positioned token per CJK char; everything else is a position gap.
#[derive(Clone, Default)]
pub(super) struct CjkUnigramTokenizer;

impl Tokenizer for CjkUnigramTokenizer {
    type TokenStream<'a> = VecTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> VecTokenStream {
        let mut tokens = Vec::new();
        let mut position = 0;
        for (offset, run, kind) in runs(text) {
            match kind {
                RunKind::Cjk => {
                    for (rel, c) in run.char_indices() {
                        // One token per *folded* char, not per source char:
                        // the digraphs ヿ and ゟ each fold to two chars, and
                        // the query side splits a folded run the same way.
                        // Both then share the char range they came from.
                        for folded in fold_query(&c.to_string()).chars() {
                            push(
                                &mut tokens,
                                &mut position,
                                offset + rel,
                                c.len_utf8(),
                                folded.to_string(),
                            );
                        }
                    }
                }
                // The gap keeps phrases from matching across non-CJK text.
                _ => position += 1,
            }
        }
        VecTokenStream {
            tokens,
            cursor: None,
        }
    }
}

/// A query analyzed the way the index is.
pub(super) struct QueryTerms {
    /// Folded non-CJK words; each must match on the word field.
    pub(super) word_musts: Vec<String>,
    /// jieba words from the query's CJK runs; optional matches on the
    /// word field that only sharpen ranking — the unigram phrase below is
    /// what decides whether a CJK run matches at all.
    pub(super) word_shoulds: Vec<String>,
    /// Each CJK run as the char sequence an `ink_uni` phrase (or, for a
    /// single char, term) query needs; every run must match.
    pub(super) cjk_runs: Vec<Vec<char>>,
}

pub(super) fn analyze_query(query: &str) -> QueryTerms {
    let mut terms = QueryTerms {
        word_musts: Vec::new(),
        word_shoulds: Vec::new(),
        cjk_runs: Vec::new(),
    };
    for (_, run, kind) in runs(query) {
        match kind {
            RunKind::Cjk => {
                terms.cjk_runs.push(fold_query(run).chars().collect());
                for token in JIEBA.cut(run, true) {
                    terms.word_shoulds.push(token.word.to_string());
                }
            }
            RunKind::Word => terms.word_musts.push(fold_query(run)),
            RunKind::Other => {}
        }
    }
    terms
}

/// A fixed probe corpus for [`analyzer_fingerprint`], wide enough that a
/// change in how text is cut shows up: jieba search-mode segmentation over
/// simplified and traditional Chinese and over Japanese, the compatibility
/// and digraph folds, half-width kana, a supplementary-plane Han char, and
/// the CJK/Latin/punctuation run boundaries.
#[cfg(test)]
const ANALYZER_PROBE: [&str; 8] = [
    "月光書房",
    "中华人民共和国万岁",
    "北京大学生前来应聘",
    "紫式部の源氏物語を読む",
    "한국어 검색 테스트",
    "ﾊﾝｶｸ ヿゟ 𠀀",
    "Café Ünïcode MIXED case",
    "混合Mixed文字123text",
];

/// Names the analysis an on-disk index was built with: the fingerprint
/// this build's tokenizers produce over `ANALYZER_PROBE`. `index.rs`
/// stamps it beside the index and discards an index carrying any other
/// value, because terms cut by a different segmenter are terms a query
/// can no longer ask for.
///
/// It is *recorded* rather than recomputed at open: comparing a string
/// costs a file read, where recomputing would load the whole jieba
/// dictionary on every library open for a value that cannot change
/// without a code change. The search tests'
/// `the_recorded_analyzer_id_is_what_the_tokenizers_produce` is what
/// keeps the record honest — a jieba bump that moves one word boundary
/// turns that test red, and updating this
/// constant is exactly what rebuilds every index already on disk. A jieba
/// bump that changes no boundary leaves it alone, so nobody pays for a
/// rebuild they did not need.
pub(super) const ANALYZER_ID: &str =
    "d81e5eca486a8eaabf67457b59b12dfcbbd5080c8d621967ba926ebcb18701ae";

/// What the *index side* of this build cuts [`ANALYZER_PROBE`] into,
/// digested: both tokenizers, every token's text, position, and char
/// range.
#[cfg(test)]
pub(super) fn analyzer_fingerprint() -> String {
    fn digest(hasher: &mut blake3::Hasher, label: &str, mut stream: VecTokenStream) {
        hasher.update(label.as_bytes());
        while stream.advance() {
            let token = stream.token();
            hasher.update(
                format!(
                    "{}\u{1}{}\u{1}{}\u{1}{}\u{2}",
                    token.text, token.position, token.offset_from, token.offset_to
                )
                .as_bytes(),
            );
        }
    }

    let mut hasher = blake3::Hasher::new();
    for probe in ANALYZER_PROBE {
        hasher.update(probe.as_bytes());
        digest(&mut hasher, "word", WordTokenizer.token_stream(probe));
        digest(&mut hasher, "uni", CjkUnigramTokenizer.token_stream(probe));
    }
    hasher.finalize().to_hex().to_string()
}
