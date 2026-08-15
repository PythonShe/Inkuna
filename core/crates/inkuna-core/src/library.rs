use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;

use crate::migrate::migrate;
use crate::pool::{open_connection, ReaderPool, READER_POOL_SIZE};
use crate::{CoreError, Format};

/// Authors are stored joined by the ASCII unit separator: it cannot appear
/// in real names, unlike commas or semicolons (common in CJK author lists).
const AUTHOR_SEP: char = '\u{1f}';

/// A library publication. `file_path` and `cover_path` are relative to the
/// library's data dir (iOS container paths change across installs, so
/// absolute paths are never persisted); the FFI layer absolutizes on read.
#[derive(Debug, Clone, PartialEq)]
pub struct Publication {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub format: Format,
    pub file_path: String,
    pub cover_path: Option<String>,
    pub added_at: i64,
    /// Book-wide `totalProgression` in [0, 1] — never per-resource.
    pub progression: f64,
    /// Current position: opaque Readium locator JSON, stored and returned,
    /// never parsed.
    pub locator: Option<String>,
    /// Readium synthetic position count, reported by the shell's navigator.
    pub position_count: Option<u32>,
    /// Non-NULL = on the Finished shelf; powers "books this year".
    pub finished_at: Option<i64>,
    pub last_opened_at: Option<i64>,
}

/// One entry of the flattened TOC. `href` (which may carry a fragment) is
/// the Readium jump target; mapping a chapter to its resource is
/// href-minus-fragment matched against the spine, derived at query time
/// when needed — never stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Chapter {
    pub id: String,
    pub idx: u32,
    pub title: String,
    pub href: String,
    pub depth: u32,
}

/// Library shelves, filtered server-side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shelf {
    /// Opened at least once and not finished.
    Reading,
    Finished,
    All,
}

/// List orderings. (A `Title` sort is deliberately omitted — no shell
/// affordance exists; enum variants are additive later.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    /// Tonight's hero is the first row.
    RecentlyOpened,
    RecentlyAdded,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    pub id: String,
    pub publication_id: String,
    /// Opaque Readium locator JSON; carries the `href`/`title`/`text`
    /// context a bookmark row renders from.
    pub locator: String,
    pub progression: f64,
    pub created_at: i64,
}

/// The library facade: one SQLite DB plus core-owned book/cover storage
/// under a single data dir. One writer connection (mutations only, each in
/// a transaction; file I/O and parsing always happen outside the lock) and
/// a fixed reader pool so reads never queue behind an import.
pub struct Library {
    pub(crate) data_dir: PathBuf,
    pub(crate) writer: Mutex<Connection>,
    pub(crate) readers: ReaderPool,
}

pub(crate) const PUB_COLUMNS: &str = "id, title, authors, language, format, file_path, \
     cover_path, added_at, progression, locator, position_count, finished_at, last_opened_at";

pub(crate) fn map_publication(row: &rusqlite::Row) -> rusqlite::Result<Publication> {
    let format_str: String = row.get(4)?;
    let format = Format::from_str(&format_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let authors: String = row.get(2)?;
    Ok(Publication {
        id: row.get(0)?,
        title: row.get(1)?,
        authors: split_authors(&authors),
        language: row.get(3)?,
        format,
        file_path: row.get(5)?,
        cover_path: row.get(6)?,
        added_at: row.get(7)?,
        progression: row.get(8)?,
        locator: row.get(9)?,
        position_count: row.get(10)?,
        finished_at: row.get(11)?,
        last_opened_at: row.get(12)?,
    })
}

impl Library {
    /// Opens (creating if needed) the library rooted at `data_dir`:
    /// `inkuna.db`, `books/`, and `covers/` all live under it and are owned
    /// by the core. Runs pending migrations, then sweeps files unreferenced
    /// by any row (crash-recovery for interrupted imports).
    ///
    /// One `Library` per `data_dir` is a hard requirement: because the sweep
    /// cannot tell an abandoned staging file from a live one, opening a
    /// second concurrent instance on the same directory deletes the first
    /// one's in-flight import.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Library, CoreError> {
        let data_dir = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(data_dir.join("books"))?;
        std::fs::create_dir_all(data_dir.join("covers"))?;

        let db_path = data_dir.join("inkuna.db");
        let mut writer = open_connection(&db_path)?;
        migrate(&mut writer, &data_dir)?;
        let readers = ReaderPool::open(&db_path, READER_POOL_SIZE)?;

        let library = Library {
            data_dir,
            writer: Mutex::new(writer),
            readers,
        };
        library.sweep()?;
        Ok(library)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn list(&self, shelf: Shelf, sort: Sort) -> Result<Vec<Publication>, CoreError> {
        let filter = match shelf {
            Shelf::Reading => "WHERE last_opened_at IS NOT NULL AND finished_at IS NULL",
            Shelf::Finished => "WHERE finished_at IS NOT NULL",
            Shelf::All => "",
        };
        let order = match sort {
            Sort::RecentlyOpened => {
                "ORDER BY last_opened_at DESC NULLS LAST, added_at DESC, rowid DESC"
            }
            Sort::RecentlyAdded => "ORDER BY added_at DESC, rowid DESC",
        };
        self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT {PUB_COLUMNS} FROM publications {filter} {order}"
            ))?;
            let rows = stmt.query_map([], map_publication)?;
            rows.collect::<Result<_, _>>().map_err(Into::into)
        })
    }

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

    pub fn add_bookmark(
        &self,
        publication_id: &str,
        locator: &str,
        progression: f64,
    ) -> Result<Bookmark, CoreError> {
        let progression = if progression.is_finite() {
            progression.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let bookmark = Bookmark {
            id: uuid::Uuid::new_v4().to_string(),
            publication_id: publication_id.to_string(),
            locator: locator.to_string(),
            progression,
            created_at: unix_now(),
        };
        let conn = self.writer.lock().unwrap();
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM publications WHERE id = ?1)",
            [publication_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(CoreError::NotFound(publication_id.to_string()));
        }
        conn.execute(
            "INSERT INTO bookmarks (id, publication_id, locator, progression, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                bookmark.id,
                bookmark.publication_id,
                bookmark.locator,
                bookmark.progression,
                bookmark.created_at,
            ],
        )?;
        Ok(bookmark)
    }

    /// Bookmarks for a publication, sorted by progression through the book.
    pub fn bookmarks(&self, publication_id: &str) -> Result<Vec<Bookmark>, CoreError> {
        self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT id, publication_id, locator, progression, created_at
                 FROM bookmarks WHERE publication_id = ?1
                 ORDER BY progression, created_at, rowid",
            )?;
            let rows = stmt.query_map([publication_id], |row| {
                Ok(Bookmark {
                    id: row.get(0)?,
                    publication_id: row.get(1)?,
                    locator: row.get(2)?,
                    progression: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?;
            rows.collect::<Result<_, _>>().map_err(Into::into)
        })
    }

    pub fn remove_bookmark(&self, bookmark_id: &str) -> Result<(), CoreError> {
        let conn = self.writer.lock().unwrap();
        let changed = conn.execute("DELETE FROM bookmarks WHERE id = ?1", [bookmark_id])?;
        if changed == 0 {
            return Err(CoreError::NotFound(bookmark_id.to_string()));
        }
        Ok(())
    }

    pub fn publication(&self, id: &str) -> Result<Publication, CoreError> {
        self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT {PUB_COLUMNS} FROM publications WHERE id = ?1"
            ))?;
            let mut rows = stmt.query_map([id], map_publication)?;
            match rows.next().transpose()? {
                Some(publication) => Ok(publication),
                None => Err(CoreError::NotFound(id.to_string())),
            }
        })
    }

    /// The flattened TOC in document order; empty for books without one
    /// (the text corpus is still complete — it keys off the spine).
    pub fn chapters(&self, id: &str) -> Result<Vec<Chapter>, CoreError> {
        let chapters: Vec<Chapter> = self.readers.with(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT id, idx, title, href, depth FROM chapters
                 WHERE publication_id = ?1 ORDER BY idx",
            )?;
            let rows = stmt.query_map([id], |row| {
                Ok(Chapter {
                    id: row.get(0)?,
                    idx: row.get(1)?,
                    title: row.get(2)?,
                    href: row.get(3)?,
                    depth: row.get(4)?,
                })
            })?;
            rows.collect::<Result<_, _>>().map_err(Into::into)
        })?;
        if chapters.is_empty() {
            // Distinguish "no TOC" from "no such publication".
            self.publication(id)?;
        }
        Ok(chapters)
    }

    /// Removes the publication row (child tables cascade), its book file,
    /// and its cover. File deletion is idempotent — missing files are not
    /// an error — and always confined to the data dir because DB paths are
    /// relative by construction.
    pub fn remove(&self, id: &str) -> Result<(), CoreError> {
        let publication = self.publication(id)?;
        {
            let conn = self.writer.lock().unwrap();
            conn.execute("DELETE FROM publications WHERE id = ?1", [id])?;
        }
        let _ = std::fs::remove_file(self.data_dir.join(&publication.file_path));
        if let Some(cover) = &publication.cover_path {
            let _ = std::fs::remove_file(self.data_dir.join(cover));
        }
        Ok(())
    }

    /// Deletes files under `books/` and `covers/` that no publication row
    /// references — leftovers of imports interrupted between the file
    /// rename and the DB commit — plus stray `.tmp` staging files.
    fn sweep(&self) -> Result<(), CoreError> {
        let referenced: HashSet<String> = self.readers.with(|conn| {
            let mut stmt =
                conn.prepare("SELECT file_path, cover_path FROM publications")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?;
            let mut set = HashSet::new();
            for row in rows {
                let (file_path, cover_path) = row?;
                set.insert(file_path);
                if let Some(cover) = cover_path {
                    set.insert(cover);
                }
            }
            Ok(set)
        })?;

        for sub in ["books", "covers"] {
            for entry in std::fs::read_dir(self.data_dir.join(sub))? {
                let entry = entry?;
                if !entry.file_type()?.is_file() {
                    continue;
                }
                let rel = format!("{sub}/{}", entry.file_name().to_string_lossy());
                if !referenced.contains(&rel) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}

pub(crate) fn join_authors(authors: &[String]) -> String {
    authors.join(&AUTHOR_SEP.to_string())
}

fn split_authors(joined: &str) -> Vec<String> {
    if joined.is_empty() {
        Vec::new()
    } else {
        joined.split(AUTHOR_SEP).map(str::to_string).collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ImportOutcome;
    use std::io::Write;

    #[derive(Clone, Copy, PartialEq)]
    pub(crate) enum TocKind {
        Nav,
        Ncx,
        None,
    }

    /// Builds a valid EPUB exercising the full import pipeline: two CJK
    /// spine chapters, a nested TOC (nav doc or NCX), and a cover image.
    pub(crate) fn write_epub_with(
        path: &Path,
        title: &str,
        author: &str,
        language: &str,
        toc: TocKind,
        cover: bool,
    ) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        zip.start_file("mimetype", stored).unwrap();
        zip.write_all(b"application/epub+zip").unwrap();

        zip.start_file("META-INF/container.xml", stored).unwrap();
        zip.write_all(
            br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
        )
        .unwrap();

        let mut manifest = String::from(
            r#"<item id="c1" href="text/ch01.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="text/ch02.xhtml" media-type="application/xhtml+xml"/>"#,
        );
        let mut spine_attr = String::new();
        match toc {
            TocKind::Nav => manifest.push_str(
                r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
            ),
            TocKind::Ncx => {
                manifest.push_str(
                    r#"<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
                );
                spine_attr = r#" toc="ncx""#.to_string();
            }
            TocKind::None => {}
        }
        if cover {
            manifest.push_str(
                r#"<item id="cover-img" href="images/cover.png" media-type="image/png" properties="cover-image"/>"#,
            );
        }

        zip.start_file("OEBPS/content.opf", stored).unwrap();
        zip.write_all(
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="uid">test</dc:identifier>
    <dc:title>{title}</dc:title>
    <dc:creator>{author}</dc:creator>
    <dc:language>{language}</dc:language>
  </metadata>
  <manifest>{manifest}</manifest>
  <spine{spine_attr}><itemref idref="c1"/><itemref idref="c2"/></spine>
</package>"#
            )
            .as_bytes(),
        )
        .unwrap();

        zip.start_file("OEBPS/text/ch01.xhtml", stored).unwrap();
        zip.write_all(
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>ch01</title></head>
<body><h1 id="s1">第一章</h1><p>月の光が窓辺に落ちていた。</p></body></html>"#.as_bytes(),
        )
        .unwrap();
        zip.start_file("OEBPS/text/ch02.xhtml", stored).unwrap();
        zip.write_all(
            br#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>ch02</title></head>
<body><p>Second chapter text.</p></body></html>"#,
        )
        .unwrap();

        match toc {
            TocKind::Nav => {
                zip.start_file("OEBPS/nav.xhtml", stored).unwrap();
                zip.write_all(
                    r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
<nav epub:type="toc"><ol>
  <li><a href="text/ch01.xhtml">第一章</a>
    <ol><li><a href="text/ch01.xhtml#s1">第一節</a></li></ol>
  </li>
  <li><a href="text/ch02.xhtml">第二章</a></li>
</ol></nav></body></html>"#.as_bytes(),
                )
                .unwrap();
            }
            TocKind::Ncx => {
                zip.start_file("OEBPS/toc.ncx", stored).unwrap();
                zip.write_all(
                    r#"<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap>
<navPoint id="a"><navLabel><text>第一章</text></navLabel><content src="text/ch01.xhtml"/>
  <navPoint id="b"><navLabel><text>第一節</text></navLabel><content src="text/ch01.xhtml#s1"/></navPoint>
</navPoint>
<navPoint id="c"><navLabel><text>第二章</text></navLabel><content src="text/ch02.xhtml"/></navPoint>
</navMap></ncx>"#.as_bytes(),
                )
                .unwrap();
            }
            TocKind::None => {}
        }

        if cover {
            zip.start_file("OEBPS/images/cover.png", stored).unwrap();
            zip.write_all(b"\x89PNG\r\n\x1a\nfake png bytes").unwrap();
        }
        zip.finish().unwrap();
    }

    /// The default fixture: nav TOC + cover.
    pub(crate) fn write_epub(path: &Path, title: &str, author: &str, language: &str) {
        write_epub_with(path, title, author, language, TocKind::Nav, true);
    }

    fn write_cbz(path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("001.jpg", stored).unwrap();
        zip.write_all(&[0xFF, 0xD8, 0xFF]).unwrap();
        zip.finish().unwrap();
    }

    /// Builds a minimal PalmDB "BOOKMOBI" file whose MOBI header carries the
    /// given file version (6 = classic MOBI, 8 = KF8/AZW3).
    fn write_mobi(path: &Path, version: u32) {
        let mut bytes = vec![0u8; 78];
        bytes[60..68].copy_from_slice(b"BOOKMOBI");
        bytes[76..78].copy_from_slice(&1u16.to_be_bytes());
        let record0_offset = 78 + 8;
        bytes.extend_from_slice(&(record0_offset as u32).to_be_bytes());
        bytes.extend_from_slice(&[0u8; 4]);
        let mut record0 = [0u8; 40];
        record0[16..20].copy_from_slice(b"MOBI");
        record0[36..40].copy_from_slice(&version.to_be_bytes());
        bytes.extend_from_slice(&record0);
        std::fs::write(path, bytes).unwrap();
    }

    fn imported(outcome: ImportOutcome) -> Publication {
        match outcome {
            ImportOutcome::Imported(p) => p,
            ImportOutcome::Duplicate(p) => panic!("expected fresh import, got duplicate of {}", p.id),
        }
    }

    #[test]
    fn detects_formats_by_content() {
        let dir = tempfile::tempdir().unwrap();

        let epub = dir.path().join("misnamed.zip");
        write_epub(&epub, "T", "A", "en");
        assert_eq!(Format::detect(&epub).unwrap(), Format::Epub);

        let cbz = dir.path().join("comic.cbz");
        write_cbz(&cbz);
        assert_eq!(Format::detect(&cbz).unwrap(), Format::Cbz);

        let rar = dir.path().join("comic.cbr");
        std::fs::write(&rar, b"Rar!\x1a\x07\x01\x00rest").unwrap();
        assert_eq!(Format::detect(&rar).unwrap(), Format::Cbr);

        let pdf = dir.path().join("paper.pdf");
        std::fs::write(&pdf, b"%PDF-1.7\n...").unwrap();
        assert_eq!(Format::detect(&pdf).unwrap(), Format::Pdf);

        let mobi = dir.path().join("classic.mobi");
        write_mobi(&mobi, 6);
        assert_eq!(Format::detect(&mobi).unwrap(), Format::Mobi);

        let azw3 = dir.path().join("modern.azw3");
        write_mobi(&azw3, 8);
        assert_eq!(Format::detect(&azw3).unwrap(), Format::Azw3);

        // TXT is extension-gated (no magic exists) but rejects binary
        // content; GB18030-style non-UTF-8 text must still pass.
        let txt = dir.path().join("web-novel.txt");
        std::fs::write(&txt, [0xB5, 0xDA, 0xD2, 0xBB, 0xD5, 0xC2]).unwrap();
        assert_eq!(Format::detect(&txt).unwrap(), Format::Txt);

        let fake_txt = dir.path().join("binary.txt");
        std::fs::write(&fake_txt, b"text\x00then binary").unwrap();
        assert!(matches!(
            Format::detect(&fake_txt),
            Err(CoreError::UnsupportedFormat(None))
        ));

        let junk = dir.path().join("junk.bin");
        std::fs::write(&junk, b"not a book").unwrap();
        assert!(matches!(
            Format::detect(&junk),
            Err(CoreError::UnsupportedFormat(None))
        ));
    }

    #[test]
    fn imports_cjk_epub_and_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let epub = dir.path().join("moonlight.epub");
        write_epub(&epub, "月光書房", "紫式部", "ja");

        let data_dir = dir.path().join("library");
        let library = Library::open(&data_dir).unwrap();
        let publication = imported(library.import(epub.to_str().unwrap()).unwrap());
        assert_eq!(publication.title, "月光書房");
        assert_eq!(publication.authors, vec!["紫式部"]);
        assert_eq!(publication.language.as_deref(), Some("ja"));
        assert_eq!(publication.format, Format::Epub);

        // The file was copied into core-owned storage under a relative path.
        assert_eq!(publication.file_path, format!("books/{}.epub", publication.id));
        assert!(data_dir.join(&publication.file_path).is_file());

        let listed = library.list(Shelf::All, Sort::RecentlyAdded).unwrap();
        assert_eq!(listed, vec![publication.clone()]);
        assert_eq!(library.publication(&publication.id).unwrap(), publication);

        library
            .update_progress(&publication.id, "{}", 0.42, None)
            .unwrap();
        assert_eq!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap()[0].progression, 0.42);

        let cover_path = publication.cover_path.clone().unwrap();
        assert!(data_dir.join(&cover_path).is_file());

        library.remove(&publication.id).unwrap();
        assert!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().is_empty());
        assert!(!data_dir.join(&publication.file_path).exists());
        assert!(!data_dir.join(&cover_path).exists());
        assert!(matches!(
            library.remove(&publication.id),
            Err(CoreError::NotFound(_))
        ));
    }

    fn count(library: &Library, sql: &str, id: &str) -> i64 {
        library
            .readers
            .with(|conn| conn.query_row(sql, [id], |row| row.get(0)).map_err(Into::into))
            .unwrap()
    }

    #[test]
    fn import_extracts_spine_toc_cover_and_corpus() {
        let dir = tempfile::tempdir().unwrap();
        let epub = dir.path().join("book.epub");
        write_epub(&epub, "月光書房", "紫式部", "ja");

        let data_dir = dir.path().join("library");
        let library = Library::open(&data_dir).unwrap();
        let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

        // Cover extracted as-is under covers/.
        assert_eq!(
            publication.cover_path.as_deref(),
            Some(format!("covers/{}.png", publication.id).as_str())
        );

        // Flattened TOC with CJK titles, fragments, and depth.
        let chapters = library.chapters(&publication.id).unwrap();
        let brief: Vec<(&str, &str, u32)> = chapters
            .iter()
            .map(|c| (c.title.as_str(), c.href.as_str(), c.depth))
            .collect();
        assert_eq!(
            brief,
            vec![
                ("第一章", "OEBPS/text/ch01.xhtml", 0),
                ("第一節", "OEBPS/text/ch01.xhtml#s1", 1),
                ("第二章", "OEBPS/text/ch02.xhtml", 0),
            ]
        );
        assert_eq!(chapters.iter().map(|c| c.idx).collect::<Vec<_>>(), vec![0, 1, 2]);

        // The spine landed in reading order with one text row per resource.
        assert_eq!(
            count(&library, "SELECT COUNT(*) FROM resources WHERE publication_id = ?1", &publication.id),
            2
        );
        let first_body: String = library
            .readers
            .with(|conn| {
                conn.query_row(
                    "SELECT rt.body FROM resource_text rt
                     JOIN resources r ON r.id = rt.resource_id
                     WHERE r.publication_id = ?1 ORDER BY r.spine_idx LIMIT 1",
                    [&publication.id],
                    |row| row.get(0),
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert!(first_body.contains("月の光が窓辺に落ちていた。"));

        // remove() cascades the children.
        library.remove(&publication.id).unwrap();
        assert_eq!(
            count(&library, "SELECT COUNT(*) FROM chapters WHERE publication_id = ?1", &publication.id),
            0
        );
        assert_eq!(
            count(&library, "SELECT COUNT(*) FROM resources WHERE publication_id = ?1", &publication.id),
            0
        );
    }

    #[test]
    fn no_toc_epub_still_builds_a_complete_corpus() {
        let dir = tempfile::tempdir().unwrap();
        let epub = dir.path().join("book.epub");
        write_epub_with(&epub, "無目次", "作者", "ja", TocKind::None, false);

        let library = Library::open(dir.path().join("library")).unwrap();
        let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

        assert!(library.chapters(&publication.id).unwrap().is_empty());
        assert!(publication.cover_path.is_none());
        // The corpus keys off the spine, not the TOC.
        let text_rows = count(
            &library,
            "SELECT COUNT(*) FROM resource_text WHERE resource_id IN
                 (SELECT id FROM resources WHERE publication_id = ?1)",
            &publication.id,
        );
        assert_eq!(text_rows, 2);
    }

    #[test]
    fn ncx_fallback_supplies_the_toc() {
        let dir = tempfile::tempdir().unwrap();
        let epub = dir.path().join("book.epub");
        write_epub_with(&epub, "旧式目次", "作者", "ja", TocKind::Ncx, false);

        let library = Library::open(dir.path().join("library")).unwrap();
        let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

        let chapters = library.chapters(&publication.id).unwrap();
        assert_eq!(chapters.len(), 3);
        assert_eq!(chapters[1].title, "第一節");
        assert_eq!(chapters[1].href, "OEBPS/text/ch01.xhtml#s1");
        assert_eq!(chapters[1].depth, 1);
    }

    #[test]
    fn rejects_non_epub_naming_the_format() {
        let dir = tempfile::tempdir().unwrap();
        let cbz = dir.path().join("鬼滅の刃 第1巻.cbz");
        write_cbz(&cbz);

        let library = Library::open(dir.path().join("library")).unwrap();
        let err = library.import(cbz.to_str().unwrap()).unwrap_err();
        match err {
            CoreError::UnsupportedFormat(Some(format)) => assert_eq!(format, "cbz"),
            other => panic!("expected UnsupportedFormat with name, got {other:?}"),
        }
        assert!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().is_empty());
    }

    #[test]
    fn import_is_idempotent_by_content_hash() {
        let dir = tempfile::tempdir().unwrap();
        let epub = dir.path().join("book.epub");
        write_epub(&epub, "月光書房", "紫式部", "ja");

        let data_dir = dir.path().join("library");
        let library = Library::open(&data_dir).unwrap();
        let first = imported(library.import(epub.to_str().unwrap()).unwrap());

        // Same file again.
        match library.import(epub.to_str().unwrap()).unwrap() {
            ImportOutcome::Duplicate(p) => assert_eq!(p.id, first.id),
            other => panic!("expected duplicate, got {other:?}"),
        }

        // Byte-identical copy under another name.
        let copy = dir.path().join("renamed-copy.epub");
        std::fs::copy(&epub, &copy).unwrap();
        match library.import(copy.to_str().unwrap()).unwrap() {
            ImportOutcome::Duplicate(p) => assert_eq!(p.id, first.id),
            other => panic!("expected duplicate, got {other:?}"),
        }

        assert_eq!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(), 1);
        // No stray files: exactly one book in storage.
        let books: Vec<_> = std::fs::read_dir(data_dir.join("books"))
            .unwrap()
            .collect();
        assert_eq!(books.len(), 1);
    }

    #[test]
    fn shelves_and_sorts_partition_the_library() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.epub");
        write_epub(&first, "最初の本", "著者A", "ja");
        let second = dir.path().join("second.epub");
        write_epub_with(&second, "Second Book", "Author B", "en", TocKind::None, false);

        let library = Library::open(dir.path().join("library")).unwrap();
        let first = imported(library.import(first.to_str().unwrap()).unwrap());
        let second = imported(library.import(second.to_str().unwrap()).unwrap());

        // Nothing opened yet: Reading and Finished are empty, All has both.
        assert!(library.list(Shelf::Reading, Sort::RecentlyAdded).unwrap().is_empty());
        assert!(library.list(Shelf::Finished, Sort::RecentlyAdded).unwrap().is_empty());
        assert_eq!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(), 2);

        // Opening puts a book on the Reading shelf and makes it the
        // recently-opened hero.
        library.session_start(&first.id).unwrap();
        let reading = library.list(Shelf::Reading, Sort::RecentlyOpened).unwrap();
        assert_eq!(reading.len(), 1);
        assert_eq!(reading[0].id, first.id);
        let by_opened = library.list(Shelf::All, Sort::RecentlyOpened).unwrap();
        assert_eq!(by_opened[0].id, first.id);
        assert_eq!(by_opened[1].id, second.id);

        // Finishing moves it off Reading onto Finished.
        library.set_finished(&first.id, true).unwrap();
        assert!(library.list(Shelf::Reading, Sort::RecentlyAdded).unwrap().is_empty());
        let finished = library.list(Shelf::Finished, Sort::RecentlyAdded).unwrap();
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].id, first.id);
    }

    #[test]
    fn search_library_matches_cjk_and_casefolds() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.epub");
        write_epub(&a, "月光書房", "紫式部", "ja");
        let b = dir.path().join("b.epub");
        write_epub_with(&b, "Die Straße", "Hans Müller", "de", TocKind::None, false);

        let library = Library::open(dir.path().join("library")).unwrap();
        library.import(a.to_str().unwrap()).unwrap();
        library.import(b.to_str().unwrap()).unwrap();

        // CJK substring over title and author.
        assert_eq!(library.search_library("月光").unwrap().len(), 1);
        assert_eq!(library.search_library("紫").unwrap().len(), 1);
        // Full Unicode case folding, not ASCII LOWER: ß ≡ ss.
        assert_eq!(library.search_library("STRASSE").unwrap().len(), 1);
        assert_eq!(library.search_library("müller").unwrap().len(), 1);
        // No match, and blank queries return nothing.
        assert!(library.search_library("存在しない").unwrap().is_empty());
        assert!(library.search_library("   ").unwrap().is_empty());
    }

    #[test]
    fn bookmarks_roundtrip_sorted_by_progression() {
        let dir = tempfile::tempdir().unwrap();
        let epub = dir.path().join("book.epub");
        write_epub(&epub, "月光書房", "紫式部", "ja");

        let library = Library::open(dir.path().join("library")).unwrap();
        let publication = imported(library.import(epub.to_str().unwrap()).unwrap());

        let late = library
            .add_bookmark(&publication.id, r#"{"locations":{"totalProgression":0.8}}"#, 0.8)
            .unwrap();
        let early = library
            .add_bookmark(&publication.id, r#"{"locations":{"totalProgression":0.2}}"#, 0.2)
            .unwrap();

        let listed = library.bookmarks(&publication.id).unwrap();
        assert_eq!(listed, vec![early.clone(), late.clone()]);

        library.remove_bookmark(&late.id).unwrap();
        assert_eq!(library.bookmarks(&publication.id).unwrap(), vec![early]);
        assert!(matches!(
            library.remove_bookmark(&late.id),
            Err(CoreError::NotFound(_))
        ));
        assert!(matches!(
            library.add_bookmark("missing", "{}", 0.5),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn batch_import_reports_per_item_outcomes_in_order() {
        use crate::BatchImportOutcome;

        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good.epub");
        write_epub(&good, "月光書房", "紫式部", "ja");
        let comic = dir.path().join("comic.cbz");
        write_cbz(&comic);
        let copy = dir.path().join("copy.epub");
        std::fs::copy(&good, &copy).unwrap();

        let library = Library::open(dir.path().join("library")).unwrap();
        let outcomes = library.import_batch(&[
            good.to_str().unwrap().to_string(),
            comic.to_str().unwrap().to_string(),
            copy.to_str().unwrap().to_string(),
        ]);

        assert_eq!(outcomes.len(), 3);
        // Identical content in one batch: exactly one Imported, the copy a
        // Duplicate of it (whichever parallel branch wins the race).
        let (imported, duplicate) = match (&outcomes[0], &outcomes[2]) {
            (BatchImportOutcome::Imported(a), BatchImportOutcome::Duplicate(b))
            | (BatchImportOutcome::Duplicate(b), BatchImportOutcome::Imported(a)) => (a, b),
            other => panic!("expected one Imported + one Duplicate, got {other:?}"),
        };
        assert_eq!(imported.id, duplicate.id);
        match &outcomes[1] {
            BatchImportOutcome::Failed { path, error } => {
                assert_eq!(path, comic.to_str().unwrap());
                assert!(matches!(error, CoreError::UnsupportedFormat(Some(f)) if f == "cbz"));
            }
            other => panic!("expected Failed for the comic, got {other:?}"),
        }
        assert_eq!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(), 1);
    }

    #[test]
    fn migration_adopts_live_rows_and_drops_dead_ones() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("library");
        std::fs::create_dir_all(&data_dir).unwrap();

        let alive = dir.path().join("alive.epub");
        write_epub(&alive, "生きてる本", "著者", "ja");

        // Hand-build a v1 database: external absolute paths, schema v1.
        {
            let conn = Connection::open(data_dir.join("inkuna.db")).unwrap();
            conn.execute_batch(
                "CREATE TABLE publications (
                    id          TEXT PRIMARY KEY,
                    title       TEXT NOT NULL,
                    authors     TEXT NOT NULL DEFAULT '',
                    language    TEXT,
                    format      TEXT NOT NULL,
                    file_path   TEXT NOT NULL,
                    added_at    INTEGER NOT NULL,
                    progression REAL NOT NULL DEFAULT 0
                );",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO publications VALUES ('live-id', '生きてる本', '著者', 'ja', 'epub', ?1, 100, 0.5)",
                [alive.to_str().unwrap()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO publications VALUES ('dead-id', 'Gone', '', NULL, 'epub', '/no/such/file.epub', 200, 0.0)",
                [],
            )
            .unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
        }

        let library = Library::open(&data_dir).unwrap();
        let listed = library.list(Shelf::All, Sort::RecentlyAdded).unwrap();
        assert_eq!(listed.len(), 1);
        let adopted = &listed[0];
        assert_eq!(adopted.id, "live-id");
        assert_eq!(adopted.title, "生きてる本");
        assert_eq!(adopted.progression, 0.5);
        // Adopted: copied in, relativized, hashed (dedupe now works on it).
        assert_eq!(adopted.file_path, "books/live-id.epub");
        assert!(data_dir.join("books/live-id.epub").is_file());
        match library.import(alive.to_str().unwrap()).unwrap() {
            ImportOutcome::Duplicate(p) => assert_eq!(p.id, "live-id"),
            other => panic!("expected duplicate of adopted row, got {other:?}"),
        }
    }

    #[test]
    fn open_sweeps_unreferenced_and_tmp_files() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("library");

        let epub = dir.path().join("book.epub");
        write_epub(&epub, "Kept", "A", "en");
        let kept = {
            let library = Library::open(&data_dir).unwrap();
            imported(library.import(epub.to_str().unwrap()).unwrap())
        };

        // Simulate a crash between file rename and DB commit, plus staging
        // leftovers and an orphaned cover.
        std::fs::write(data_dir.join("books/orphan.epub"), b"orphan").unwrap();
        std::fs::write(data_dir.join("books/half.epub.tmp"), b"partial").unwrap();
        std::fs::write(data_dir.join("covers/orphan.jpg"), b"img").unwrap();

        let library = Library::open(&data_dir).unwrap();
        assert!(!data_dir.join("books/orphan.epub").exists());
        assert!(!data_dir.join("books/half.epub.tmp").exists());
        assert!(!data_dir.join("covers/orphan.jpg").exists());
        // The referenced book survived.
        assert!(data_dir.join(&kept.file_path).is_file());
        assert_eq!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(), 1);
    }

    #[test]
    fn reads_do_not_queue_behind_the_writer() {
        let dir = tempfile::tempdir().unwrap();
        let epub = dir.path().join("book.epub");
        write_epub(&epub, "T", "A", "en");

        let library = Library::open(dir.path().join("library")).unwrap();
        library.import(epub.to_str().unwrap()).unwrap();

        // Hold the writer lock; a read must still complete — if list()
        // touched the writer connection this would deadlock the test.
        let _writer_held = library.writer.lock().unwrap();
        assert_eq!(library.list(Shelf::All, Sort::RecentlyAdded).unwrap().len(), 1);
    }
}
