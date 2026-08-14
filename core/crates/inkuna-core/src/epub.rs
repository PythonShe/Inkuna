//! Minimal EPUB metadata extraction: container.xml -> OPF -> Dublin Core.
//! Full publication parsing (spine, resources, positions) is Readium's job
//! in the shells; the core only needs enough to populate the library shelf.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::CoreError;

#[derive(Debug, Default)]
pub struct EpubMetadata {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
}

pub fn read_metadata(path: &Path) -> Result<EpubMetadata, CoreError> {
    let mut archive = zip::ZipArchive::new(File::open(path)?)?;

    let container = read_entry(&mut archive, "META-INF/container.xml")?;
    let opf_path = rootfile_path(&container)?;
    let opf = read_entry(&mut archive, &opf_path)?;
    Ok(parse_opf(&opf))
}

fn read_entry(archive: &mut zip::ZipArchive<File>, name: &str) -> Result<String, CoreError> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| CoreError::InvalidPublication(format!("missing {name}")))?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf)?;
    Ok(buf)
}

fn rootfile_path(container_xml: &str) -> Result<String, CoreError> {
    let mut reader = Reader::from_str(container_xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == b"rootfile" => {
                for attr in e.attributes().flatten() {
                    if attr.key.local_name().as_ref() == b"full-path" {
                        return Ok(String::from_utf8_lossy(&attr.value).into_owned());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(CoreError::InvalidPublication(e.to_string())),
            _ => {}
        }
        buf.clear();
    }
    Err(CoreError::InvalidPublication("no rootfile in container.xml".into()))
}

fn parse_opf(opf_xml: &str) -> EpubMetadata {
    let mut meta = EpubMetadata::default();
    let mut reader = Reader::from_str(opf_xml);
    let mut buf = Vec::new();
    // Tracks which dc: element we are inside so the next text node is routed
    // to the right field. Only the first title/language wins.
    let mut current: Option<&'static str> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                current = match e.local_name().as_ref() {
                    b"title" => Some("title"),
                    b"creator" => Some("creator"),
                    b"language" => Some("language"),
                    _ => None,
                };
            }
            Ok(Event::Text(t)) => {
                if let Some(field) = current {
                    let text = t.unescape().unwrap_or_default().trim().to_string();
                    if text.is_empty() {
                        continue;
                    }
                    match field {
                        "title" if meta.title.is_none() => meta.title = Some(text),
                        "creator" => meta.authors.push(text),
                        "language" if meta.language.is_none() => meta.language = Some(text),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(_)) => current = None,
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    meta
}
