//! Metadata search over the library: title and authors, CJK-safe.

use super::model::{Publication, Shelf, Sort};
use super::Library;
use crate::CoreError;

impl Library {
    /// Case-folded, CJK-safe substring search over title and authors,
    /// matched in Rust with full Unicode case folding (ICU4X) — never SQL
    /// `LOWER`, which is ASCII-only. Metadata search only; in-book
    /// full-text search is the search spec's engine over this spec's
    /// corpus.
    pub fn search_library(&self, query: &str) -> Result<Vec<Publication>, CoreError> {
        let mapper = icu_casemap::CaseMapper::new();
        let needle = mapper.fold_string(query.trim());
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        let all = self.list(Shelf::All, Sort::RecentlyAdded)?;
        Ok(all
            .into_iter()
            .filter(|p| {
                mapper.fold_string(&p.title).contains(needle.as_ref())
                    || p.authors
                        .iter()
                        .any(|a| mapper.fold_string(a).contains(needle.as_ref()))
            })
            .collect())
    }
}
