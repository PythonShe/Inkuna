//! A small EPUB 3 writer shared by the reflowable-format converters.

use std::borrow::Cow;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use quick_xml::escape::escape;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use inkuna_content::{MAX_SPINE_ENTRY_BYTES, MAX_SPINE_ITEMS, MAX_TOC_ENTRIES};
use crate::CoreError;

const DEFAULT_STYLESHEET: &str = "p { text-indent: 2em; margin: 0.2em 0; }\n\
hr.scene { border: none; text-align: center; margin: 1.5em 0; }\n\
h1, h2, h3, h4, h5, h6 { text-align: center; }\n";
const CHAPTER_BEFORE_TITLE: &str =
    r#"<!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml"><head><title>"#;
const CHAPTER_AFTER_TITLE: &str =
    r#"</title><link rel="stylesheet" type="text/css" href="../style.css"/></head><body>"#;
const CHAPTER_AFTER_BODY: &str = "</body></html>";

struct Chapter {
    title: String,
    body: String,
}

struct Volume {
    title: String,
    chapters: Vec<Chapter>,
}

enum Section {
    Chapter(Chapter),
    Volume(Volume),
}

struct Image {
    name: String,
    bytes: Vec<u8>,
    mime: String,
}

struct CoverImage {
    bytes: Vec<u8>,
    mime: String,
}

/// Builds one deterministic EPUB 3 archive from trusted XHTML block content.
///
/// Mutators retain the first writer-limit error and [`finish`](Self::finish)
/// returns it. This keeps the converter-facing builder API simple while
/// preventing any further content from being retained after a cap trips.
#[allow(dead_code)]
pub(crate) struct EpubWriter {
    title: String,
    authors: Vec<String>,
    language: String,
    extra_stylesheet: String,
    cover: Option<CoverImage>,
    images: Vec<Image>,
    sections: Vec<Section>,
    chapter_count: usize,
    toc_count: usize,
    error: Option<CoreError>,
}

#[allow(dead_code)]
impl EpubWriter {
    pub(crate) fn new(title: &str) -> Self {
        Self {
            title: title.into(),
            authors: Vec::new(),
            language: "und".into(),
            extra_stylesheet: String::new(),
            cover: None,
            images: Vec::new(),
            sections: Vec::new(),
            chapter_count: 0,
            toc_count: 0,
            error: None,
        }
    }

    pub(crate) fn author(&mut self, name: &str) {
        self.authors.push(name.into());
    }

    pub(crate) fn language(&mut self, tag: &str) {
        self.language = tag.into();
    }

    /// Appends converter-specific rules after the default book stylesheet.
    pub(crate) fn stylesheet(&mut self, css: &str) {
        if !self.extra_stylesheet.is_empty() {
            self.extra_stylesheet.push('\n');
        }
        self.extra_stylesheet.push_str(css);
        self.extra_stylesheet.push('\n');
    }

    pub(crate) fn set_cover(&mut self, bytes: Vec<u8>, mime: &str) {
        self.cover = Some(CoverImage {
            bytes,
            mime: mime.into(),
        });
    }

    /// Adds an image and returns its href relative to a chapter document.
    pub(crate) fn add_image(&mut self, name: &str, bytes: Vec<u8>, mime: &str) -> String {
        let href = format!("../images/{name}");
        self.images.push(Image {
            name: name.into(),
            bytes,
            mime: mime.into(),
        });
        href
    }

    /// Opens a volume group. Chapters added afterwards belong to it until
    /// another volume starts.
    pub(crate) fn begin_volume(&mut self, title: &str) {
        if self.error.is_some() {
            return;
        }
        if self.toc_count == MAX_TOC_ENTRIES {
            self.error = Some(CoreError::InvalidPublication(format!(
                "EPUB writer TOC exceeds {MAX_TOC_ENTRIES} entries"
            )));
            return;
        }
        self.toc_count += 1;
        self.sections.push(Section::Volume(Volume {
            title: title.into(),
            chapters: Vec::new(),
        }));
    }

    /// Adds trusted, pre-escaped XHTML block content as one spine item.
    pub(crate) fn add_chapter(&mut self, title: &str, body_xhtml: &str) {
        if self.error.is_some() {
            return;
        }
        if body_xhtml.len() as u64 > MAX_SPINE_ENTRY_BYTES {
            self.error = Some(CoreError::InvalidPublication(format!(
                "EPUB writer chapter body exceeds {MAX_SPINE_ENTRY_BYTES} bytes"
            )));
            return;
        }
        if self.chapter_count == MAX_SPINE_ITEMS {
            self.error = Some(CoreError::InvalidPublication(format!(
                "EPUB writer spine exceeds {MAX_SPINE_ITEMS} items"
            )));
            return;
        }
        if self.toc_count == MAX_TOC_ENTRIES {
            self.error = Some(CoreError::InvalidPublication(format!(
                "EPUB writer TOC exceeds {MAX_TOC_ENTRIES} entries"
            )));
            return;
        }

        self.chapter_count += 1;
        self.toc_count += 1;
        let chapter = Chapter {
            title: title.into(),
            body: body_xhtml.into(),
        };
        match self.sections.last_mut() {
            Some(Section::Volume(volume)) => volume.chapters.push(chapter),
            _ => self.sections.push(Section::Chapter(chapter)),
        }
    }

    pub(crate) fn finish(self, dst: &Path) -> Result<(), CoreError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let empty_volumes = self
            .sections
            .iter()
            .filter(
                |section| matches!(section, Section::Volume(volume) if volume.chapters.is_empty()),
            )
            .count();
        if self.chapter_count + empty_volumes > MAX_SPINE_ITEMS {
            return Err(CoreError::InvalidPublication(format!(
                "EPUB writer spine exceeds {MAX_SPINE_ITEMS} items"
            )));
        }

        let identifier = deterministic_identifier(&self);
        let documents = documents(&self.sections);
        if documents.is_empty() {
            return Err(CoreError::InvalidPublication(
                "EPUB writer requires at least one spine item".into(),
            ));
        }
        if documents
            .iter()
            .any(|document| chapter_xhtml_len(document) > MAX_SPINE_ENTRY_BYTES as usize)
        {
            return Err(CoreError::InvalidPublication(format!(
                "EPUB writer spine resource exceeds {MAX_SPINE_ENTRY_BYTES} bytes after XHTML wrapping"
            )));
        }
        let file = File::create(dst)?;
        let mut archive = ZipWriter::new(file);

        write_entry(
            &mut archive,
            "mimetype",
            b"application/epub+zip",
            CompressionMethod::Stored,
        )?;
        write_entry(
            &mut archive,
            "META-INF/container.xml",
            container_xml().as_bytes(),
            CompressionMethod::Deflated,
        )?;
        let opf = content_opf(&self, &documents, &identifier);
        write_entry(
            &mut archive,
            "OEBPS/content.opf",
            opf.as_bytes(),
            CompressionMethod::Deflated,
        )?;
        let nav = nav_xhtml(&self.sections);
        write_entry(
            &mut archive,
            "OEBPS/nav.xhtml",
            nav.as_bytes(),
            CompressionMethod::Deflated,
        )?;

        let mut stylesheet = String::from(DEFAULT_STYLESHEET);
        stylesheet.push_str(&self.extra_stylesheet);
        write_entry(
            &mut archive,
            "OEBPS/style.css",
            stylesheet.as_bytes(),
            CompressionMethod::Deflated,
        )?;
        for (index, document) in documents.iter().enumerate() {
            let xhtml = chapter_xhtml(&document.title, &document.body);
            write_entry(
                &mut archive,
                &chapter_path(index + 1),
                xhtml.as_bytes(),
                CompressionMethod::Deflated,
            )?;
        }
        for image in &self.images {
            write_entry(
                &mut archive,
                &format!("OEBPS/images/{}", image.name),
                &image.bytes,
                CompressionMethod::Deflated,
            )?;
        }
        if let Some(cover) = &self.cover {
            write_entry(
                &mut archive,
                &format!("OEBPS/images/cover.{}", cover_extension(&cover.mime)),
                &cover.bytes,
                CompressionMethod::Deflated,
            )?;
        }
        archive.finish()?;
        Ok(())
    }
}

struct Document<'a> {
    title: &'a str,
    body: Cow<'a, str>,
}

fn documents(sections: &[Section]) -> Vec<Document<'_>> {
    let mut documents = Vec::new();
    for section in sections {
        match section {
            Section::Chapter(chapter) => documents.push(Document {
                title: &chapter.title,
                body: Cow::Borrowed(&chapter.body),
            }),
            Section::Volume(volume) if volume.chapters.is_empty() => {
                documents.push(Document {
                    title: &volume.title,
                    body: Cow::Owned(format!("<h1>{}</h1>", escape(&volume.title))),
                });
            }
            Section::Volume(volume) => {
                documents.extend(volume.chapters.iter().map(|chapter| Document {
                    title: &chapter.title,
                    body: Cow::Borrowed(&chapter.body),
                }));
            }
        }
    }
    documents
}

fn deterministic_identifier(writer: &EpubWriter) -> uuid::Uuid {
    fn field(hasher: &mut blake3::Hasher, value: &[u8]) {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value);
    }
    let mut hasher = blake3::Hasher::new();
    field(&mut hasher, writer.title.as_bytes());
    for author in &writer.authors {
        field(&mut hasher, author.as_bytes());
    }
    field(&mut hasher, writer.language.as_bytes());
    field(&mut hasher, writer.extra_stylesheet.as_bytes());
    if let Some(cover) = &writer.cover {
        field(&mut hasher, cover.mime.as_bytes());
        field(&mut hasher, &cover.bytes);
    }
    for image in &writer.images {
        field(&mut hasher, image.name.as_bytes());
        field(&mut hasher, image.mime.as_bytes());
        field(&mut hasher, &image.bytes);
    }
    for section in &writer.sections {
        match section {
            Section::Chapter(chapter) => {
                field(&mut hasher, b"chapter");
                hash_chapter(&mut hasher, chapter);
            }
            Section::Volume(volume) => {
                field(&mut hasher, b"volume");
                field(&mut hasher, volume.title.as_bytes());
                for chapter in &volume.chapters {
                    hash_chapter(&mut hasher, chapter);
                }
            }
        }
    }
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes)
}

fn hash_chapter(hasher: &mut blake3::Hasher, chapter: &Chapter) {
    hasher.update(&(chapter.title.len() as u64).to_le_bytes());
    hasher.update(chapter.title.as_bytes());
    hasher.update(&(chapter.body.len() as u64).to_le_bytes());
    hasher.update(chapter.body.as_bytes());
}

fn container_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#
}

fn content_opf(writer: &EpubWriter, documents: &[Document<'_>], identifier: &uuid::Uuid) -> String {
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id" prefix="dcterms: http://purl.org/dc/terms/"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="pub-id">urn:uuid:{identifier}</dc:identifier><dc:title>{}</dc:title>"#,
        escape(&writer.title)
    );
    for author in &writer.authors {
        xml.push_str(&format!("<dc:creator>{}</dc:creator>", escape(author)));
    }
    xml.push_str(&format!(
        "<dc:language>{}</dc:language><meta property=\"dcterms:modified\">1970-01-01T00:00:00Z</meta></metadata><manifest>",
        escape(&writer.language)
    ));
    xml.push_str(
        "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/><item id=\"style\" href=\"style.css\" media-type=\"text/css\"/>",
    );
    for index in 1..=documents.len() {
        xml.push_str(&format!("<item id=\"ch{index:05}\" href=\"text/ch{index:05}.xhtml\" media-type=\"application/xhtml+xml\"/>"));
    }
    for (index, image) in writer.images.iter().enumerate() {
        xml.push_str(&format!(
            "<item id=\"img{index:05}\" href=\"images/{}\" media-type=\"{}\"/>",
            escape(&image.name),
            escape(&image.mime)
        ));
    }
    if let Some(cover) = &writer.cover {
        xml.push_str(&format!(
            "<item id=\"cover-image\" href=\"images/cover.{}\" media-type=\"{}\" properties=\"cover-image\"/>",
            cover_extension(&cover.mime),
            escape(&cover.mime)
        ));
    }
    xml.push_str("</manifest><spine>");
    for index in 1..=documents.len() {
        xml.push_str(&format!("<itemref idref=\"ch{index:05}\"/>"));
    }
    xml.push_str("</spine></package>");
    xml
}

fn nav_xhtml(sections: &[Section]) -> String {
    let mut xml = String::from(
        r#"<!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title><link rel="stylesheet" type="text/css" href="style.css"/></head><body><nav epub:type="toc"><ol>"#,
    );
    let mut index = 1usize;
    for section in sections {
        match section {
            Section::Chapter(chapter) => {
                xml.push_str(&format!(
                    "<li><a href=\"text/ch{index:05}.xhtml\">{}</a></li>",
                    escape(&chapter.title)
                ));
                index += 1;
            }
            Section::Volume(volume) if volume.chapters.is_empty() => {
                xml.push_str(&format!(
                    "<li><a href=\"text/ch{index:05}.xhtml\">{}</a></li>",
                    escape(&volume.title)
                ));
                index += 1;
            }
            Section::Volume(volume) => {
                xml.push_str(&format!(
                    "<li><a href=\"text/ch{index:05}.xhtml\">{}</a><ol>",
                    escape(&volume.title)
                ));
                for chapter in &volume.chapters {
                    xml.push_str(&format!(
                        "<li><a href=\"text/ch{index:05}.xhtml\">{}</a></li>",
                        escape(&chapter.title)
                    ));
                    index += 1;
                }
                xml.push_str("</ol></li>");
            }
        }
    }
    xml.push_str("</ol></nav></body></html>");
    xml
}

fn chapter_xhtml(title: &str, body: &str) -> String {
    format!(
        "{CHAPTER_BEFORE_TITLE}{}{CHAPTER_AFTER_TITLE}{body}{CHAPTER_AFTER_BODY}",
        escape(title),
    )
}

fn chapter_xhtml_len(document: &Document<'_>) -> usize {
    CHAPTER_BEFORE_TITLE.len()
        + escape(document.title).len()
        + CHAPTER_AFTER_TITLE.len()
        + document.body.len()
        + CHAPTER_AFTER_BODY.len()
}

fn chapter_path(index: usize) -> String {
    format!("OEBPS/text/ch{index:05}.xhtml")
}

fn cover_extension(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        "image/webp" => "webp",
        _ => "img",
    }
}

fn write_entry(
    archive: &mut ZipWriter<File>,
    name: &str,
    bytes: &[u8],
    compression: CompressionMethod,
) -> Result<(), CoreError> {
    let options = SimpleFileOptions::default()
        .compression_method(compression)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    archive.start_file(name, options)?;
    archive.write_all(bytes)?;
    Ok(())
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
