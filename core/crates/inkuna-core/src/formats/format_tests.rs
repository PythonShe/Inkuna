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

#[test]
fn detects_utf16_txt_but_still_rejects_nul_binary_data() {
    let dir = tempfile::tempdir().unwrap();
    let bom = dir.path().join("bom.txt");
    let mut bytes = vec![0xff, 0xfe];
    bytes.extend(
        "第一章 Chapter 1\n山中 Body"
            .encode_utf16()
            .flat_map(u16::to_le_bytes),
    );
    std::fs::write(&bom, bytes).unwrap();
    assert_eq!(Format::detect(&bom).unwrap(), Format::Txt);

    let bomless = dir.path().join("bomless.txt");
    let bytes: Vec<u8> = "Chapter 1\nBody"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    std::fs::write(&bomless, bytes).unwrap();
    assert_eq!(Format::detect(&bomless).unwrap(), Format::Txt);

    let binary = dir.path().join("nul-heavy.txt");
    std::fs::write(&binary, [0, 1, 0, 2, 3, 0, 4, 0]).unwrap();
    assert!(matches!(
        Format::detect(&binary),
        Err(CoreError::UnsupportedFormat(None))
    ));
}

#[test]
fn detects_bomless_utf16_cjk_txt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cjk.txt");
    let text = "春夏秋冬 山中月夜".repeat(64);
    let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    std::fs::write(&path, bytes).unwrap();

    assert_eq!(Format::detect(&path).unwrap(), Format::Txt);
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

fn utf16(text: &str, little_endian: bool) -> Vec<u8> {
    if little_endian {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    } else {
        text.encode_utf16().flat_map(u16::to_be_bytes).collect()
    }
}

fn assert_detected_as_txt_in_both_byte_orders(dir: &Path, text: &str) {
    for (name, little_endian) in [("le.txt", true), ("be.txt", false)] {
        let path = dir.join(name);
        std::fs::write(&path, utf16(text, little_endian)).unwrap();
        assert_eq!(Format::detect(&path).unwrap(), Format::Txt, "{name}");
    }
}

/// CJK UTF-16 carries its NULs at the opposite parity from ASCII UTF-16,
/// because they come from the low byte of U+xx00 characters (一 U+4E00,
/// 言 U+8A00, 退 U+9000). Detection must accept both byte orders, and the
/// converter must agree on which one — otherwise mojibake reaches
/// `resource_text`, the search index, and the reader.
#[test]
fn detects_bomless_utf16_cjk_with_low_byte_nul_characters() {
    let dir = tempfile::tempdir().unwrap();
    let text = "一片寂静，月光洒在窗台上。".repeat(64);
    assert_detected_as_txt_in_both_byte_orders(dir.path(), &text);
}

/// Smart quotes and em dashes keep a UTF-16 sample byte-wise valid UTF-8
/// without making it pure-ASCII UTF-16. Such a file must still be detected
/// as TXT rather than failing import outright.
#[test]
fn detects_bomless_utf16_english_with_smart_punctuation() {
    let dir = tempfile::tempdir().unwrap();
    let text = "It was the best of times — the ‘worst’ too. ".repeat(64);
    assert_detected_as_txt_in_both_byte_orders(dir.path(), &text);
}
