//! Import pipeline: one streaming pass copies the source into core-owned
//! storage while hashing it (BLAKE3), dedupes on the hash, parses the copy,
//! then commits atomically — rename first, DB transaction second, so a
//! crash leaves an unreferenced file (swept at next open), never a fileless
//! row. Invariant: a committed row always points at an existing file.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::epub;
use crate::library::{join_authors, map_publication, unix_now, Library, PUB_COLUMNS};
use crate::{CoreError, Format, Publication};

#[derive(Debug, Clone, PartialEq)]
pub enum ImportOutcome {
    Imported(Publication),
    Duplicate(Publication),
}

/// A fully parsed import, ready to commit: the file already sits at
/// `tmp_path` and every DB value is computed. Parsing happens outside any
/// lock; committing takes the writer.
pub(crate) struct PreparedImport {
    id: String,
    tmp_path: PathBuf,
    rel_path: String,
    content_hash: String,
    title: String,
    authors: Vec<String>,
    language: Option<String>,
    /// Spine hrefs in reading order, paired with each resource's extracted
    /// plain text (`None` = malformed resource, its text row is skipped).
    spine: Vec<(String, Option<String>)>,
    toc: Vec<epub::TocEntry>,
    cover: Option<epub::Cover>,
}

pub(crate) enum Prepared {
    Duplicate(Publication),
    Fresh(Box<PreparedImport>),
}

impl Library {
    pub fn import(&self, path: &str) -> Result<ImportOutcome, CoreError> {
        match self.prepare_import(path)? {
            Prepared::Duplicate(existing) => Ok(ImportOutcome::Duplicate(existing)),
            Prepared::Fresh(prepared) => self.commit_import(*prepared),
        }
    }

    /// Streams the source into a `.tmp` under `books/` while hashing,
    /// checks the hash against the library, and parses the copy. No writer
    /// lock is held at any point.
    pub(crate) fn prepare_import(&self, path: &str) -> Result<Prepared, CoreError> {
        let src = Path::new(path);
        let format = Format::detect(src)?;
        if format != Format::Epub {
            return Err(CoreError::UnsupportedFormat(Some(format.as_str().to_string())));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let rel_path = format!("books/{id}.epub");
        let tmp_path = self.data_dir.join(format!("books/{id}.epub.tmp"));

        let content_hash = copy_and_hash(src, &tmp_path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp_path);
        })?;

        if let Some(existing) = self.publication_by_hash(&content_hash)? {
            let _ = std::fs::remove_file(&tmp_path);
            return Ok(Prepared::Duplicate(existing));
        }

        let parsed = epub::read_package(&tmp_path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp_path);
        })?;
        let title = parsed
            .metadata
            .title
            .or_else(|| src.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .ok_or_else(|| CoreError::InvalidPublication("untitled".into()))?;

        // Rayon across resources: the corpus keys off the spine, so it is
        // complete even for books with no TOC.
        let texts = epub::extract_spine_text(&tmp_path, &parsed.spine);
        let spine = parsed.spine.into_iter().zip(texts).collect();

        Ok(Prepared::Fresh(Box::new(PreparedImport {
            id,
            tmp_path,
            rel_path,
            content_hash,
            title,
            authors: parsed.metadata.authors,
            language: parsed.metadata.language,
            spine,
            toc: parsed.toc,
            cover: parsed.cover,
        })))
    }

    /// Writes the cover, renames the staged book into place **first**, then
    /// inserts all rows in one transaction — a crash in between leaves
    /// unreferenced files (swept at next open), never a fileless row. A
    /// concurrent import of the same content loses the unique-index race
    /// and resolves to `Duplicate`.
    pub(crate) fn commit_import(&self, prepared: PreparedImport) -> Result<ImportOutcome, CoreError> {
        let final_path = self.data_dir.join(&prepared.rel_path);

        let cover_rel = match &prepared.cover {
            Some(cover) => {
                let rel = format!("covers/{}.{}", prepared.id, cover.extension);
                if let Err(e) = std::fs::write(self.data_dir.join(&rel), &cover.bytes) {
                    let _ = std::fs::remove_file(&prepared.tmp_path);
                    return Err(e.into());
                }
                Some(rel)
            }
            None => None,
        };
        let cleanup_files = |include_book: bool| {
            if include_book {
                let _ = std::fs::remove_file(&final_path);
            }
            if let Some(rel) = &cover_rel {
                let _ = std::fs::remove_file(self.data_dir.join(rel));
            }
        };

        if let Err(e) = std::fs::rename(&prepared.tmp_path, &final_path) {
            let _ = std::fs::remove_file(&prepared.tmp_path);
            cleanup_files(false);
            return Err(e.into());
        }

        let publication = Publication {
            id: prepared.id,
            title: prepared.title,
            authors: prepared.authors,
            language: prepared.language,
            format: Format::Epub,
            file_path: prepared.rel_path,
            cover_path: cover_rel.clone(),
            added_at: unix_now(),
            progression: 0.0,
            locator: None,
            position_count: None,
            finished_at: None,
            last_opened_at: None,
        };

        let insert = |tx: &rusqlite::Transaction| -> Result<(), rusqlite::Error> {
            tx.execute(
                "INSERT INTO publications
                    (id, title, authors, language, format, file_path, cover_path,
                     content_hash, added_at, progression)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    publication.id,
                    publication.title,
                    join_authors(&publication.authors),
                    publication.language,
                    publication.format.as_str(),
                    publication.file_path,
                    publication.cover_path,
                    prepared.content_hash,
                    publication.added_at,
                    publication.progression,
                ],
            )?;
            for (spine_idx, (href, text)) in prepared.spine.iter().enumerate() {
                let resource_id = uuid::Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO resources (id, publication_id, spine_idx, href)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![resource_id, publication.id, spine_idx as i64, href],
                )?;
                if let Some(body) = text {
                    tx.execute(
                        "INSERT INTO resource_text (resource_id, body) VALUES (?1, ?2)",
                        rusqlite::params![resource_id, body],
                    )?;
                }
            }
            for (idx, entry) in prepared.toc.iter().enumerate() {
                tx.execute(
                    "INSERT INTO chapters (id, publication_id, idx, title, href, depth)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        uuid::Uuid::new_v4().to_string(),
                        publication.id,
                        idx as i64,
                        entry.title,
                        entry.href,
                        entry.depth,
                    ],
                )?;
            }
            Ok(())
        };

        let inserted = {
            let mut conn = self.writer.lock().unwrap();
            let tx = conn.transaction()?;
            match insert(&tx) {
                Ok(()) => {
                    tx.commit()?;
                    true
                }
                Err(e) if is_constraint_violation(&e) => {
                    drop(tx);
                    false
                }
                Err(e) => {
                    drop(tx);
                    cleanup_files(true);
                    return Err(e.into());
                }
            }
        };

        if inserted {
            Ok(ImportOutcome::Imported(publication))
        } else {
            // Lost the race: another import committed the same content
            // between our dedupe check and this insert.
            cleanup_files(true);
            match self.publication_by_hash(&prepared.content_hash)? {
                Some(existing) => Ok(ImportOutcome::Duplicate(existing)),
                None => Err(CoreError::NotFound(prepared.content_hash)),
            }
        }
    }

    fn publication_by_hash(&self, hash: &str) -> Result<Option<Publication>, CoreError> {
        self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT {PUB_COLUMNS} FROM publications WHERE content_hash = ?1"
            ))?;
            let mut rows = stmt.query_map([hash], map_publication)?;
            rows.next().transpose().map_err(Into::into)
        })
    }
}

fn is_constraint_violation(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(err, _)
            if err.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

/// Reads `src` once, hashing with BLAKE3 while writing the bytes to `dest`.
/// The destination is fsynced so the later rename lands durable content.
pub(crate) fn copy_and_hash(src: &Path, dest: &Path) -> Result<String, CoreError> {
    let mut reader = File::open(src)?;
    let mut writer = File::create(dest)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 128 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        writer.write_all(&buf[..n])?;
    }
    writer.sync_all()?;
    Ok(hasher.finalize().to_hex().to_string())
}
