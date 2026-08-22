//! Shared test fixtures. Every test builds its own books in a tempdir —
//! no binary fixtures live in git.

use std::io::Write;
use std::path::Path;

use crate::{ImportOutcome, Publication};

#[path = "mobi_test_support.rs"]
mod mobi;

pub(crate) use mobi::{
    build_indx_records, palmdoc_compress, IndxEntryFixture, Kf8FileFixture, Kf8NcxFixture,
    MobiTestBuilder,
};
// The EPUB fixture builders moved to `inkuna-content` with the parsers;
// re-exported so existing tests keep compiling unchanged.
pub(crate) use inkuna_content::test_support::{
    write_epub, write_epub_parts, write_epub_with, CoverKind, TocKind,
};

pub(crate) fn write_cbz(path: &Path) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("001.jpg", stored).unwrap();
    zip.write_all(&[0xFF, 0xD8, 0xFF]).unwrap();
    zip.finish().unwrap();
}

/// Builds a minimal PalmDB "BOOKMOBI" file whose MOBI header carries the
/// given file version (6 = classic MOBI, 8 = KF8/AZW3).
pub(crate) fn write_mobi(path: &Path, version: u32) {
    MobiTestBuilder::new(version).write(path);
}

pub(crate) fn imported(outcome: ImportOutcome) -> Publication {
    match outcome {
        ImportOutcome::Imported(p) => p,
        ImportOutcome::Duplicate(p) => panic!("expected fresh import, got duplicate of {}", p.id),
    }
}
