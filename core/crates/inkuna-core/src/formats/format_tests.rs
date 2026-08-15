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

