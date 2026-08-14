use std::path::Path;
use std::sync::Mutex;

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
        migrate(&conn)?;
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
            // Filename fallback until each format's real support lands:
            // MOBI/AZW3 EXTH metadata arrives with the convert-to-EPUB
            // importer, PDF metadata with the PDF navigator, ComicInfo.xml
            // with comics; TXT has no embedded metadata at all.
            Format::Mobi | Format::Azw3 | Format::Txt | Format::Pdf | Format::Cbz | Format::Cbr => {
                (None, Vec::new(), None)
            }
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

/// Append-only versioned migrations tracked via SQLite's `user_version`
/// pragma. Never edit a shipped entry — add a new one. (refinery would be
/// preferred, but it currently caps rusqlite below our version; revisit.)
const MIGRATIONS: &[&str] = &[
    // 0001: initial schema
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
];

fn migrate(conn: &Connection) -> Result<(), CoreError> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    for (index, migration) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        conn.execute_batch(migration)?;
        conn.pragma_update(None, "user_version", (index + 1) as i64)?;
    }
    Ok(())
}

fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
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
            Err(CoreError::UnsupportedFormat)
        ));

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
