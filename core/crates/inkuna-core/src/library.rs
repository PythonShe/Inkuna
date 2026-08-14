use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::epub;
use crate::{CoreError, Format};

/// Authors are stored joined by the ASCII unit separator: it cannot appear
/// in real names, unlike commas or semicolons (common in CJK author lists).
const AUTHOR_SEP: char = '\u{1f}';

#[derive(Debug, Clone, PartialEq)]
pub struct Publication {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub format: Format,
    pub file_path: String,
    pub added_at: i64,
    /// Overall reading progress in [0, 1].
    pub progression: f64,
}

pub struct Library {
    conn: Mutex<Connection>,
}

impl Library {
    pub fn open(db_path: &str) -> Result<Library, CoreError> {
        let conn = Connection::open(db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS publications (
                id          TEXT PRIMARY KEY,
                title       TEXT NOT NULL,
                authors     TEXT NOT NULL DEFAULT '',
                language    TEXT,
                format      TEXT NOT NULL,
                file_path   TEXT NOT NULL,
                added_at    INTEGER NOT NULL,
                progression REAL NOT NULL DEFAULT 0
            );",
        )?;
        Ok(Library { conn: Mutex::new(conn) })
    }

    /// Registers a publication file in the library. The file itself stays
    /// where it is; copying into app storage is the shell's responsibility.
    pub fn import(&self, path: &str) -> Result<Publication, CoreError> {
        let file_path = Path::new(path);
        let format = Format::detect(file_path)?;

        let (title, authors, language) = match format {
            Format::Epub => {
                let meta = epub::read_metadata(file_path)?;
                (meta.title, meta.authors, meta.language)
            }
            // Comics carry no standard embedded metadata; fall back to the
            // filename until ComicInfo.xml support lands.
            Format::Cbz | Format::Cbr => (None, Vec::new(), None),
        };
        let title = title
            .or_else(|| {
                file_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .ok_or_else(|| CoreError::InvalidPublication("untitled".into()))?;

        let publication = Publication {
            id: uuid::Uuid::new_v4().to_string(),
            title,
            authors,
            language,
            format,
            file_path: path.to_string(),
            added_at: unix_now(),
            progression: 0.0,
        };

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO publications
                (id, title, authors, language, format, file_path, added_at, progression)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                publication.id,
                publication.title,
                join_authors(&publication.authors),
                publication.language,
                publication.format.as_str(),
                publication.file_path,
                publication.added_at,
                publication.progression,
            ],
        )?;
        Ok(publication)
    }

    pub fn list(&self) -> Result<Vec<Publication>, CoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, authors, language, format, file_path, added_at, progression
             FROM publications ORDER BY added_at DESC, rowid DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, f64>(7)?,
            ))
        })?;

        let mut publications = Vec::new();
        for row in rows {
            let (id, title, authors, language, format, file_path, added_at, progression) = row?;
            publications.push(Publication {
                id,
                title,
                authors: split_authors(&authors),
                language,
                format: Format::from_str(&format)?,
                file_path,
                added_at,
                progression,
            });
        }
        Ok(publications)
    }

    pub fn set_progression(&self, id: &str, progression: f64) -> Result<(), CoreError> {
        let progression = progression.clamp(0.0, 1.0);
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE publications SET progression = ?1 WHERE id = ?2",
            rusqlite::params![progression, id],
        )?;
        if changed == 0 {
            return Err(CoreError::NotFound(id.to_string()));
        }
        Ok(())
    }

    pub fn remove(&self, id: &str) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM publications WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(CoreError::NotFound(id.to_string()));
        }
        Ok(())
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn join_authors(authors: &[String]) -> String {
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
mod tests {
    use super::*;
    use std::io::Write;

    /// Builds a minimal but valid EPUB with the given Dublin Core fields.
    fn write_epub(path: &Path, title: &str, author: &str, language: &str) {
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
  <manifest/><spine/>
</package>"#
            )
            .as_bytes(),
        )
        .unwrap();
        zip.finish().unwrap();
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

        let junk = dir.path().join("junk.bin");
        std::fs::write(&junk, b"not a book").unwrap();
        assert!(matches!(
            Format::detect(&junk),
            Err(CoreError::UnsupportedFormat)
        ));
    }

    #[test]
    fn imports_cjk_epub_and_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let epub = dir.path().join("moonlight.epub");
        write_epub(&epub, "月光書房", "紫式部", "ja");

        let library = Library::open(dir.path().join("library.db").to_str().unwrap()).unwrap();
        let imported = library.import(epub.to_str().unwrap()).unwrap();
        assert_eq!(imported.title, "月光書房");
        assert_eq!(imported.authors, vec!["紫式部"]);
        assert_eq!(imported.language.as_deref(), Some("ja"));
        assert_eq!(imported.format, Format::Epub);

        let listed = library.list().unwrap();
        assert_eq!(listed, vec![imported.clone()]);

        library.set_progression(&imported.id, 0.42).unwrap();
        assert_eq!(library.list().unwrap()[0].progression, 0.42);

        library.remove(&imported.id).unwrap();
        assert!(library.list().unwrap().is_empty());
        assert!(matches!(
            library.remove(&imported.id),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn comic_title_falls_back_to_filename() {
        let dir = tempfile::tempdir().unwrap();
        let cbz = dir.path().join("鬼滅の刃 第1巻.cbz");
        write_cbz(&cbz);

        let library = Library::open(dir.path().join("library.db").to_str().unwrap()).unwrap();
        let imported = library.import(cbz.to_str().unwrap()).unwrap();
        assert_eq!(imported.title, "鬼滅の刃 第1巻");
        assert_eq!(imported.format, Format::Cbz);
        assert!(imported.authors.is_empty());
    }
}
