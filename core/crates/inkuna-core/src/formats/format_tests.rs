use super::*;
use crate::test_support::{write_cbz, write_epub, write_mobi};

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

/// A `mimetype` entry that trims to the EPUB literal but inflates far past
/// the detection budget must not be read whole, and must not pass as an
/// EPUB — detection falls through exactly as a wrong mimetype string does.
#[test]
fn oversized_mimetype_entry_is_not_an_epub() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bomb.epub");
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // Deflates to a few KB; padding is whitespace so the pre-cap code's
    // `trim()` comparison would still have matched the EPUB literal.
    let mut mime = b"application/epub+zip".to_vec();
    mime.extend(std::iter::repeat_n(b' ', 4 * 1024 * 1024));
    zip.start_file("mimetype", deflated).unwrap();
    zip.write_all(&mime).unwrap();
    zip.start_file("001.jpg", deflated).unwrap();
    zip.write_all(&[0xFF, 0xD8, 0xFF]).unwrap();
    zip.finish().unwrap();

    assert_eq!(Format::detect(&path).unwrap(), Format::Cbz);
}

