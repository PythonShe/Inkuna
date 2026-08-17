//! The two search entry points on `Library`.

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, PhraseQuery, Query, TermQuery};
use tantivy::schema::IndexRecordOption;
use tantivy::Term;

use super::fold::{fold_query, fold_text};
use super::index::doc_fields;
use super::model::{BookSearchHit, BookSearchResults, LibrarySearchHit};
use super::tokenize::analyze_query;
use crate::{CoreError, Library};

/// Ranked resource docs fetched per page; pagination continues until the
/// requested number of distinct publications is collected or all matches are
/// exhausted, so one book's many matching resources cannot crowd out others.
const LIBRARY_DOC_PAGE_SIZE: usize = 256;

impl Library {
    /// Every occurrence of `query` in one book, in reading order —
    /// an exact, case-folded (NFKC + full Unicode fold) substring scan
    /// over the book's extracted text. Matches partial words ("cat" in
    /// "category") and single CJK chars by construction; any length
    /// floor is a shell UI policy, not the core's. Returns up to `limit`
    /// hits and the true total; an empty or whitespace query returns no
    /// hits. `NotFound` when the book does not exist.
    pub fn search_in_book(
        &self,
        id: &str,
        query: &str,
        limit: u32,
    ) -> Result<BookSearchResults, CoreError> {
        let needle = fold_query(query.trim());
        let rows: Vec<(u32, String, String)> = self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT r.spine_idx, r.href, t.body FROM resources r
                 JOIN resource_text t ON t.resource_id = r.id
                 WHERE r.publication_id = ?1 ORDER BY r.spine_idx",
            )?;
            let rows = stmt.query_map([id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
            rows.collect::<Result<_, _>>().map_err(Into::into)
        })?;
        if rows.is_empty() {
            // Distinguish "book with no text" from "no such book".
            self.publication(id)?;
        }
        if needle.is_empty() {
            return Ok(BookSearchResults { hits: Vec::new(), total: 0 });
        }

        let mut hits = Vec::new();
        let mut total: u32 = 0;
        for (spine_idx, href, body) in &rows {
            let folded = fold_text(body);
            let char_count = body.chars().count().max(1) as f64;
            for (start, end) in folded.occurrences(&needle) {
                total += 1;
                if hits.len() >= limit as usize {
                    continue;
                }
                let (pre, matched, post) = super::fold::snippet(body, &folded, start, end);
                hits.push(BookSearchHit {
                    spine_idx: *spine_idx,
                    href: href.clone(),
                    char_offset: start,
                    snippet_pre: pre,
                    snippet_match: matched,
                    snippet_post: post,
                    progression: f64::from(start) / char_count,
                });
            }
        }
        Ok(BookSearchResults { hits, total })
    }

    /// Ranked library-wide full-text search: which books talk about this,
    /// best first, one excerpt each. Word-level matching via the jieba
    /// field for non-CJK and ranking; exact unigram phrases for CJK runs,
    /// so a single Han char or any CJK substring matches across the whole
    /// library. Freshly imported books are searchable immediately; books
    /// indexed by a still-running background reconcile appear once it
    /// finishes.
    pub fn search_all_books(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<LibrarySearchHit>, CoreError> {
        let trimmed = query.trim();
        let terms = analyze_query(trimmed);
        if terms.word_musts.is_empty() && terms.cjk_runs.is_empty() {
            return Ok(Vec::new());
        }
        if limit == 0 {
            return Ok(Vec::new());
        }
        let fields = self.search.fields();

        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        let term_query = |field, text: &str| -> Box<dyn Query> {
            Box::new(TermQuery::new(
                Term::from_field_text(field, text),
                IndexRecordOption::WithFreqs,
            ))
        };
        for word in &terms.word_musts {
            clauses.push((Occur::Must, term_query(fields.body_word, word)));
        }
        for word in &terms.word_shoulds {
            clauses.push((Occur::Should, term_query(fields.body_word, word)));
        }
        for run in &terms.cjk_runs {
            let query: Box<dyn Query> = if run.len() == 1 {
                term_query(fields.body_uni, &run[0].to_string())
            } else {
                Box::new(PhraseQuery::new(
                    run.iter()
                        .map(|c| Term::from_field_text(fields.body_uni, &c.to_string()))
                        .collect(),
                ))
            };
            clauses.push((Occur::Must, query));
        }

        // Page through the complete ranking until enough distinct books are
        // found. A fixed top-docs cap can otherwise be exhausted by one book
        // with hundreds of matching resources.
        let searcher = self.search.searcher()?;
        let query = BooleanQuery::new(clauses);
        let mut picked: Vec<(String, u32)> = Vec::new();
        let mut offset = 0;
        loop {
            let ranked = searcher
                .search(
                    &query,
                    &TopDocs::with_limit(LIBRARY_DOC_PAGE_SIZE)
                        .and_offset(offset)
                        .order_by_score(),
                )
                .map_err(|e| CoreError::Search(e))?;
            if ranked.is_empty() {
                break;
            }
            offset += ranked.len();
            for (_score, address) in ranked {
                let (publication_id, spine_idx) = doc_fields(&searcher, fields, address)?;
                if picked.iter().any(|(id, _)| *id == publication_id) {
                    continue;
                }
                picked.push((publication_id, spine_idx));
                if picked.len() >= limit as usize {
                    break;
                }
            }
            if picked.len() >= limit as usize || offset == 0 {
                break;
            }
            // A short page means the ranking is exhausted.
            if offset % LIBRARY_DOC_PAGE_SIZE != 0 {
                break;
            }
        }

        let mut out = Vec::with_capacity(picked.len());
        for (publication_id, spine_idx) in picked {
            // The index can briefly be ahead of the database (a remove
            // between search and this read); such hits just drop out.
            let publication = match self.publication(&publication_id) {
                Ok(p) => p,
                Err(CoreError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            };
            let (excerpt_pre, excerpt_match, excerpt_post) =
                self.excerpt(&publication_id, spine_idx, trimmed, &terms)?;
            out.push(LibrarySearchHit {
                publication,
                excerpt_pre,
                excerpt_match,
                excerpt_post,
            });
        }
        Ok(out)
    }

    /// The first pinnable occurrence in the hit's best resource: the
    /// whole query if it appears verbatim, else the first CJK run or
    /// word. Word matches spread across a resource degrade to the
    /// resource's opening text with an empty match piece.
    fn excerpt(
        &self,
        publication_id: &str,
        spine_idx: u32,
        query: &str,
        terms: &super::tokenize::QueryTerms,
    ) -> Result<(String, String, String), CoreError> {
        let body: Option<String> = self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT t.body FROM resources r
                 JOIN resource_text t ON t.resource_id = r.id
                 WHERE r.publication_id = ?1 AND r.spine_idx = ?2",
            )?;
            let mut rows = stmt.query_map(
                rusqlite::params![publication_id, spine_idx],
                |row| row.get(0),
            )?;
            rows.next().transpose().map_err(Into::into)
        })?;
        let Some(body) = body else {
            return Ok((String::new(), String::new(), String::new()));
        };

        let folded = fold_text(&body);
        let mut needles = vec![fold_query(query)];
        needles.extend(terms.cjk_runs.iter().map(|run| run.iter().collect()));
        needles.extend(terms.word_musts.iter().cloned());
        needles.extend(terms.word_shoulds.iter().cloned());
        for needle in needles.into_iter().filter(|n| !n.is_empty()) {
            if let Some((start, end)) = folded.occurrences(&needle).next() {
                return Ok(super::fold::snippet(&body, &folded, start, end));
            }
        }
        // Opening text; the caps match a snippet's total window.
        let opening: String = body
            .chars()
            .take(super::fold::SNIPPET_BEFORE + super::fold::SNIPPET_AFTER)
            .collect();
        let truncated = opening.chars().count() < body.chars().count();
        Ok((
            String::new(),
            String::new(),
            format!("{}{}", opening.trim_end(), if truncated { "…" } else { "" }),
        ))
    }
}
