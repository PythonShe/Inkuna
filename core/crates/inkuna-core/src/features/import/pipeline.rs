//! The import entry points and the prepare stage: stage and hash, dedupe,
//! convert, parse. Everything here runs outside the writer lock; handing
//! the resulting [`PreparedImport`] to `commit` is what takes it.

use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};

use inkuna_content::MAX_TOTAL_TEXT_BYTES;
use inkuna_engine::extract_corpus;

use super::model::{BatchImportOutcome, ImportOutcome};
use super::restore::{HashMatch, Tombstone};
use crate::core::files::{copy_and_hash, stream_and_hash};
use crate::features::library::Library;
use crate::formats::{epub, mobi, txt};
use crate::{CoreError, Format, Publication};

/// A fully parsed import, ready to commit: the file already sits at
/// `tmp_path` and every DB value is computed. Parsing happens outside any
/// lock; committing takes the writer.
pub(crate) struct PreparedImport {
    pub(super) id: String,
    pub(super) tmp_path: PathBuf,
    pub(super) rel_path: String,
    pub(super) content_hash: String,
    pub(super) title: String,
    pub(super) authors: Vec<String>,
    pub(super) language: Option<String>,
    pub(super) text_encoding: Option<String>,
    /// Spine hrefs in reading order, paired with each resource's canonical
    /// projection text (`None` = malformed or budget-skipped resource, its
    /// text row is skipped). Repeated spine entries share one extraction.
    pub(super) spine: Vec<(String, Option<String>)>,
    pub(super) toc: Vec<epub::TocEntry>,
    pub(super) cover: Option<epub::Cover>,
    /// Set when this content matched a tombstone: `id` and `rel_path`
    /// above are then the *removed* publication's, not freshly minted,
    /// and the commit updates that row instead of inserting a new one.
    pub(super) restore: Option<Tombstone>,
}

pub(crate) enum Prepared {
    Duplicate(Box<Publication>),
    Fresh(Box<PreparedImport>),
}

impl Library {
    /// Imports one file at `path`: detect the format, copy it into
    /// core-owned storage while hashing (BLAKE3), dedupe on that hash,
    /// parse the copy for metadata, spine, TOC, cover, and text, then
    /// commit. The source file is never modified or moved.
    ///
    /// A crafted or damaged book degrades rather than failing wherever the
    /// missing part is optional — an oversized TOC is truncated, an
    /// unreadable chapter loses only its text row, an unusable cover is
    /// dropped. It accepts native EPUB plus MOBI/AZW3 and TXT normalized to
    /// EPUB, fails with `UnsupportedFormat` for fixed-layout formats
    /// today, and with `InvalidPublication` when a mandatory part is
    /// missing, no title can be derived, or the per-publication
    /// persistence budget trips (in which case the transaction rolls back
    /// and the staged file is swept). A source past the import ceiling
    /// (`files::MAX_IMPORT_BYTES`) fails with `FileTooLarge` mid-copy,
    /// its partial staged file swept.
    pub fn import(&self, path: &str) -> Result<ImportOutcome, CoreError> {
        match self.prepare_import(path)? {
            Prepared::Duplicate(existing) => Ok(ImportOutcome::Duplicate(*existing)),
            Prepared::Fresh(prepared) => self.commit_import(*prepared),
        }
    }

    /// [`import`](Self::import) over an already-open stream — a
    /// shell-owned file descriptor. Android's SAF hands out streams, not
    /// paths; this route makes the one copy into core-owned storage the
    /// only copy. `display_name` is the provider's name for the document:
    /// it drives the TXT extension check and the fallback title.
    pub fn import_reader(
        &self,
        reader: &mut dyn Read,
        display_name: &str,
    ) -> Result<ImportOutcome, CoreError> {
        match self.prepare_reader_import(reader, display_name)? {
            Prepared::Duplicate(existing) => Ok(ImportOutcome::Duplicate(*existing)),
            Prepared::Fresh(prepared) => self.commit_import(*prepared),
        }
    }

    /// Imports many files, parallelizing the copy/hash/parse stage with
    /// rayon; DB commits serialize per-item on the writer, which is fine
    /// because parsing dominates. Reuses the single-import pipeline
    /// verbatim; outcomes come back in input order. Two identical files in
    /// one batch resolve to Imported + Duplicate via the unique-index race.
    pub fn import_batch(&self, paths: &[String]) -> Vec<BatchImportOutcome> {
        self.import_batch_with(paths, &|_, _| {})
    }

    /// [`import_batch`](Self::import_batch) reporting progress: `on_done`
    /// fires once per finished file with the count of files done so far
    /// (including that one) and the input path that finished. Counts are
    /// strictly increasing across calls, but the paths need not arrive in
    /// input order — files finish in parallel. Called from rayon worker
    /// threads, under an internal lock that keeps the counts ordered, so
    /// keep the callback quick.
    pub fn import_batch_with(
        &self,
        paths: &[String],
        on_done: &(dyn Fn(usize, &str) + Sync),
    ) -> Vec<BatchImportOutcome> {
        use rayon::prelude::*;
        let done = std::sync::Mutex::new(0usize);
        paths
            .par_iter()
            .map(|path| {
                let outcome = match self.import(path) {
                    Ok(outcome) => outcome.into(),
                    Err(error) => BatchImportOutcome::Failed {
                        path: path.clone(),
                        error,
                    },
                };
                {
                    // The callback runs under the lock so a consumer never
                    // sees the counter go backwards.
                    let mut done = done.lock().unwrap();
                    *done += 1;
                    on_done(*done, path);
                }
                outcome
            })
            .collect()
    }

    /// [`import_batch_with`](Self::import_batch_with) over open streams:
    /// per-item `(reader, display_name)` pairs, outcomes in input order,
    /// with `Failed.path` and the progress callback carrying the display
    /// name — the only name a stream has.
    pub fn import_batch_readers<R: Read + Send>(
        &self,
        items: Vec<(R, String)>,
        on_done: &(dyn Fn(usize, &str) + Sync),
    ) -> Vec<BatchImportOutcome> {
        use rayon::prelude::*;
        let done = std::sync::Mutex::new(0usize);
        items
            .into_par_iter()
            .map(|(mut reader, name)| {
                let outcome = match self.import_reader(&mut reader, &name) {
                    Ok(outcome) => outcome.into(),
                    Err(error) => BatchImportOutcome::Failed {
                        path: name.clone(),
                        error,
                    },
                };
                {
                    let mut done = done.lock().unwrap();
                    *done += 1;
                    on_done(*done, &name);
                }
                outcome
            })
            .collect()
    }

    /// Streams the source into a `.tmp` under `books/` while hashing,
    /// checks the hash against the library, and parses the copy. No writer
    /// lock is held at any point.
    pub(crate) fn prepare_import(&self, path: &str) -> Result<Prepared, CoreError> {
        let src = Path::new(path);
        // Detect before copying, so a wrong-format file is rejected
        // without paying for a copy of it.
        let format = Format::detect(src)?;
        if !matches!(
            format,
            Format::Epub | Format::Mobi | Format::Azw3 | Format::Txt
        ) {
            return Err(CoreError::UnsupportedFormat(Some(
                format.as_str().to_string(),
            )));
        }

        let (id, rel_path, tmp_path) = self.stage_slot();
        let content_hash = copy_and_hash(src, &tmp_path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp_path);
        })?;
        // Dedupe on the original bytes before conversion: the hash never
        // changes, so a duplicate must not pay for a MOBI/AZW3/TXT convert.
        // A *tombstone* match is not a duplicate — the staged file is kept
        // and the whole import runs, onto the removed book's id.
        if let HashMatch::Live(existing) = self.match_staged(&content_hash, &tmp_path)? {
            return Ok(Prepared::Duplicate(existing));
        }
        let text_encoding = if format == Format::Epub {
            None
        } else {
            self.convert_staged(format, &id, &tmp_path, src.file_stem())?
        };
        self.prepare_staged(
            id,
            rel_path,
            tmp_path,
            content_hash,
            src.file_stem(),
            text_encoding,
        )
    }

    /// [`prepare_import`](Self::prepare_import) over an open stream. The
    /// stream is drained into core-owned storage *first* — it may be an
    /// unseekable pipe — so format detection runs on the copy, after the
    /// copy; a wrong-format stream costs one copy that is then swept.
    fn prepare_reader_import(
        &self,
        reader: &mut dyn Read,
        display_name: &str,
    ) -> Result<Prepared, CoreError> {
        let (id, rel_path, tmp_path) = self.stage_slot();
        let named = Path::new(display_name);

        let content_hash = stream_and_hash(reader, &tmp_path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp_path);
        })?;
        let format = Format::detect_as(&tmp_path, named).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp_path);
        })?;
        if !matches!(
            format,
            Format::Epub | Format::Mobi | Format::Azw3 | Format::Txt
        ) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(CoreError::UnsupportedFormat(Some(
                format.as_str().to_string(),
            )));
        }
        // Same pre-conversion dedupe as `prepare_import`.
        if let HashMatch::Live(existing) = self.match_staged(&content_hash, &tmp_path)? {
            return Ok(Prepared::Duplicate(existing));
        }
        let text_encoding = if format == Format::Epub {
            None
        } else {
            self.convert_staged(format, &id, &tmp_path, named.file_stem())?
        };
        self.prepare_staged(
            id,
            rel_path,
            tmp_path,
            content_hash,
            named.file_stem(),
            text_encoding,
        )
    }

    /// One fresh staging slot under `books/`: `(id, rel_path, tmp_path)`.
    fn stage_slot(&self) -> (String, String, PathBuf) {
        let id = uuid::Uuid::new_v4().to_string();
        let rel_path = format!("books/{id}.epub");
        let tmp_path = self.data_dir.join(format!("books/{id}.epub.tmp"));
        (id, rel_path, tmp_path)
    }

    /// Replaces staged source bytes with their normalized EPUB while the
    /// content hash continues to identify the original bytes.
    ///
    /// That asymmetry is why a restore cannot trust its stored coordinates
    /// on sight: the hash that matched the tombstone describes the source
    /// file, and what the coordinates index is whatever *this* build's
    /// conversion and projection make of it. The corpus digest is what
    /// closes the gap; see `restore`.
    fn convert_staged(
        &self,
        format: Format,
        id: &str,
        tmp_path: &Path,
        fallback_stem: Option<&OsStr>,
    ) -> Result<Option<String>, CoreError> {
        let conv_path = self.data_dir.join(format!("books/{id}.epub.conv.tmp"));
        let title = fallback_stem
            .map(|stem| nfc(&stem.to_string_lossy()))
            .filter(|title| !title.is_empty());
        let conversion: Result<Option<String>, CoreError> = match format {
            Format::Txt => title
                .ok_or_else(|| CoreError::InvalidPublication("untitled".into()))
                .and_then(|title| {
                    txt::convert_to_epub(tmp_path, &conv_path, &title).map_err(CoreError::from)
                })
                .map(|conversion| Some(conversion.encoding)),
            Format::Mobi | Format::Azw3 => {
                mobi::convert_to_epub(tmp_path, &conv_path, title.as_deref().unwrap_or(""))
                    .map(|()| None)
                    .map_err(CoreError::from)
            }
            _ => unreachable!(),
        };
        let conversion = match conversion {
            Ok(conversion) => conversion,
            Err(error) => {
                let _ = std::fs::remove_file(tmp_path);
                let _ = std::fs::remove_file(&conv_path);
                return Err(error);
            }
        };
        if let Err(error) = std::fs::File::open(&conv_path).and_then(|file| file.sync_all()) {
            let _ = std::fs::remove_file(tmp_path);
            let _ = std::fs::remove_file(&conv_path);
            return Err(error.into());
        }
        if let Err(error) = std::fs::rename(&conv_path, tmp_path) {
            let _ = std::fs::remove_file(tmp_path);
            let _ = std::fs::remove_file(&conv_path);
            return Err(error.into());
        }
        Ok(conversion)
    }

    /// The back half both sources share once the bytes sit staged: dedupe
    /// on the hash, parse the copy, and assemble the `PreparedImport`.
    /// `fallback_stem` names the book when its metadata cannot.
    fn prepare_staged(
        &self,
        id: String,
        rel_path: String,
        tmp_path: PathBuf,
        content_hash: String,
        fallback_stem: Option<&OsStr>,
        text_encoding: Option<String>,
    ) -> Result<Prepared, CoreError> {
        // Re-checked here even though both callers dedupe before conversion:
        // this closes the window for the reader path, where another import of
        // the same content may have committed since the pre-conversion check.
        // This is also where a tombstone is adopted — after conversion, so
        // the id and destination path below are the ones the commit uses.
        let (id, rel_path, restore) = match self.match_staged(&content_hash, &tmp_path)? {
            HashMatch::Live(existing) => return Ok(Prepared::Duplicate(existing)),
            HashMatch::Removed(tombstone) => {
                // The freshly minted id is dropped in favour of the removed
                // book's: everything preserved at removal hangs off that
                // one. The staging `.tmp` keeps its own fresh name, so two
                // concurrent restores of the same content never stage onto
                // each other. The clone is unavoidable — `tombstone` is
                // carried on for its corpus digest.
                let id = tombstone.id.clone();
                let rel_path = format!("books/{id}.epub");
                (id, rel_path, Some(tombstone))
            }
            HashMatch::Fresh => (id, rel_path, None),
        };

        let parsed = epub::read_package(&tmp_path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp_path);
        })?;
        let title = parsed
            .metadata
            .title
            .or_else(|| fallback_stem.map(|s| nfc(&s.to_string_lossy())))
            .ok_or_else(|| {
                let _ = std::fs::remove_file(&tmp_path);
                CoreError::InvalidPublication("untitled".into())
            })?;

        // The corpus is THE canonical projection: `resource_text` rows are
        // exactly `extract_corpus` output, so search offsets and layout
        // offsets index the same stream by construction. Keyed off the
        // spine, so it is complete even for books with no TOC.
        let hrefs: Vec<String> = parsed.spine.into_iter().map(|item| item.href).collect();
        let texts = extract_corpus(&tmp_path, &hrefs, MAX_TOTAL_TEXT_BYTES);
        let spine = hrefs.into_iter().zip(texts).collect();

        Ok(Prepared::Fresh(Box::new(PreparedImport {
            id,
            tmp_path,
            rel_path,
            content_hash,
            title,
            authors: parsed.metadata.authors,
            language: parsed.metadata.language,
            text_encoding,
            spine,
            toc: parsed.toc,
            // Normalized here, in the parallel parse stage, so the commit
            // under the writer lock only ever writes display-sized bytes.
            cover: parsed.cover.map(super::cover::normalize_cover),
            restore,
        })))
    }
}

/// Normalizes a filename-derived title to NFC. File providers hand back
/// decomposed forms on some volumes (HFS+/APFS most of all), and a
/// decomposed CJK or Hangul title renders identically but compares, sorts,
/// and searches differently from every composed string in the library.
/// Titles from inside a book are left verbatim — this is only for names the
/// filesystem made up. Living here, no shell can forget it.
fn nfc(name: &str) -> String {
    icu_normalizer::ComposingNormalizer::new_nfc()
        .normalize(name)
        .into_owned()
}
