use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use inkuna_content::test_support::EpubBuilder;
use inkuna_content::{deobfuscate, read_package, ObfuscationScheme};
use tempfile::TempDir;

use super::extract_publisher_fonts;
use crate::error::EngineError;
use crate::fonts::{FontRegistry, PublisherFaceSpec, FIRST_DYNAMIC_ID};
use crate::session::{EngineSession, LayoutEvents, Viewport};
use crate::settings::LayoutSettings;

const UUID_ID: &str = "urn:uuid:12345678-90ab-cdef-1234-567890abcdef";

/// The real shipped bytes of a small bundled face, reused as the
/// embedded-font payload (assets are product files, not fixtures).
fn font_bytes() -> Vec<u8> {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../assets/fonts/NotoSerifHebrew-Regular.ttf"
    ));
    std::fs::read(path).unwrap_or_else(|e| panic!("bundled face must read: {e}"))
}

const CHAPTER: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title>
<link rel="stylesheet" type="text/css" href="styles.css"/></head>
<body><p>שלום font-face world</p></body></html>"#;

const CSS: &str = r#"
@font-face {
  font-family: "Pub Face";
  font-weight: 400;
  src: url(fonts/pub.ttf);
}
p { font-family: "Pub Face", serif; }
"#;

/// A one-chapter book embedding one font, declared via @font-face.
fn font_book(font: &[u8]) -> EpubBuilder {
    EpubBuilder::new()
        .resource("ch01.xhtml", "application/xhtml+xml", CHAPTER.as_bytes())
        .resource("styles.css", "text/css", CSS.as_bytes())
        .resource("fonts/pub.ttf", "font/ttf", font)
        .spine(&["ch01.xhtml"])
}

fn extract(dir: &TempDir, builder: EpubBuilder) -> Vec<PublisherFaceSpec> {
    let epub = dir.path().join("book.epub");
    builder.write(&epub);
    let package = read_package(&epub).expect("package parses");
    extract_publisher_fonts(&epub, &package, &dir.path().join("pubfonts"))
}

#[test]
fn font_face_declared_font_extracts_with_declared_identity() {
    let dir = TempDir::new().unwrap();
    let specs = extract(&dir, font_book(&font_bytes()));
    assert_eq!(specs.len(), 1);
    let spec = &specs[0];
    assert_eq!(spec.family, "Pub Face");
    assert!(!spec.italic);
    assert_eq!(spec.weight, (400, 400));
    assert!(spec.file_path.exists());
    assert_eq!(
        spec.file_path.extension().and_then(|e| e.to_str()),
        Some("ttf")
    );
    assert_eq!(std::fs::read(&spec.file_path).unwrap(), font_bytes());
}

/// A second extraction reuses the content-hash-named file instead of
/// rewriting it.
#[test]
fn extraction_is_idempotent_and_content_addressed() {
    let dir = TempDir::new().unwrap();
    let first = extract(&dir, font_book(&font_bytes()));
    let mtime = std::fs::metadata(&first[0].file_path).unwrap().modified().unwrap();
    let second = extract(&dir, font_book(&font_bytes()));
    assert_eq!(first, second);
    assert_eq!(
        std::fs::metadata(&second[0].file_path).unwrap().modified().unwrap(),
        mtime,
        "the cached file must be reused, not rewritten"
    );
}

/// Obfuscated fonts round-trip through both standard schemes: the book
/// declares the scheme in META-INF/encryption.xml and the extracted
/// file holds the ORIGINAL bytes.
#[test]
fn obfuscated_fonts_deobfuscate_via_encryption_xml() {
    for (scheme, algorithm) in [
        (ObfuscationScheme::Idpf, "http://www.idpf.org/2008/embedding"),
        (ObfuscationScheme::Adobe, "http://ns.adobe.com/pdf/enc#RC"),
    ] {
        let mut mangled = font_bytes();
        // XOR is symmetric: "deobfuscating" clean bytes obfuscates them.
        assert!(deobfuscate(&mut mangled, scheme, Some(UUID_ID)));
        let encryption = format!(
            r#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
 xmlns:enc="http://www.w3.org/2001/04/xmlenc#">
<enc:EncryptedData><enc:EncryptionMethod Algorithm="{algorithm}"/>
<enc:CipherData><enc:CipherReference URI="OEBPS/fonts/pub.ttf"/></enc:CipherData>
</enc:EncryptedData></encryption>"#
        );
        let dir = TempDir::new().unwrap();
        let specs = extract(
            &dir,
            font_book(&mangled)
                .identifier(UUID_ID)
                .encryption_xml(&encryption),
        );
        assert_eq!(specs.len(), 1, "{scheme:?}");
        assert_eq!(
            std::fs::read(&specs[0].file_path).unwrap(),
            font_bytes(),
            "{scheme:?} deobfuscation must restore the original font"
        );
    }
}

/// An obfuscated font whose key cannot be derived (Adobe scheme, no
/// UUID identifier) is skipped — scrambled bytes never register.
#[test]
fn underivable_key_skips_the_face() {
    let mut mangled = font_bytes();
    assert!(deobfuscate(
        &mut mangled,
        ObfuscationScheme::Adobe,
        Some(UUID_ID)
    ));
    let encryption = r#"<encryption
 xmlns:enc="http://www.w3.org/2001/04/xmlenc#">
<enc:EncryptedData><enc:EncryptionMethod Algorithm="http://ns.adobe.com/pdf/enc#RC"/>
<enc:CipherData><enc:CipherReference URI="OEBPS/fonts/pub.ttf"/></enc:CipherData>
</enc:EncryptedData></encryption>"#;
    let dir = TempDir::new().unwrap();
    // Identifier stays the default "fixture": not a UUID.
    let specs = extract(&dir, font_book(&mangled).encryption_xml(encryption));
    assert!(specs.is_empty());
}

/// A WOFF-wrapped font decompresses to raw sfnt before registration —
/// the cached file must parse as ttf, not woff.
#[test]
fn woff1_wrapped_font_decompresses_to_sfnt() {
    let sfnt = font_bytes();
    let woff = wrap_woff1_stored(&sfnt);
    let dir = TempDir::new().unwrap();
    let book = EpubBuilder::new()
        .resource("ch01.xhtml", "application/xhtml+xml", CHAPTER.as_bytes())
        .resource(
            "styles.css",
            "text/css",
            CSS.replace("fonts/pub.ttf", "fonts/pub.woff").as_bytes(),
        )
        .resource("fonts/pub.woff", "font/woff", &woff)
        .spine(&["ch01.xhtml"]);
    let specs = extract(&dir, book);
    assert_eq!(specs.len(), 1);
    let cached = std::fs::read(&specs[0].file_path).unwrap();
    assert_ne!(&cached[..4], b"wOFF");
    assert!(
        read_fonts::FontRef::new(&cached).is_ok(),
        "the cached file must be a parseable sfnt"
    );
}

/// A manifest font item no rule references still registers, under the
/// font's own name-table identity.
#[test]
fn manifest_only_fonts_register_with_introspected_identity() {
    let dir = TempDir::new().unwrap();
    let book = EpubBuilder::new()
        .resource("ch01.xhtml", "application/xhtml+xml", CHAPTER.as_bytes())
        .resource("fonts/pub.ttf", "application/vnd.ms-opentype", &font_bytes())
        .spine(&["ch01.xhtml"]);
    let specs = extract(&dir, book);
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].family, "Noto Serif Hebrew");
    assert!(!specs[0].italic);
    assert_eq!(specs[0].weight, (400, 400));
}

/// Garbage declared as a font is validated away, and a book with no
/// fonts extracts nothing.
#[test]
fn invalid_fonts_and_fontless_books_yield_nothing() {
    let dir = TempDir::new().unwrap();
    let specs = extract(&dir, font_book(b"not a font at all"));
    assert!(specs.is_empty());

    let dir = TempDir::new().unwrap();
    let plain = EpubBuilder::new()
        .resource("ch01.xhtml", "application/xhtml+xml", CHAPTER.as_bytes())
        .spine(&["ch01.xhtml"]);
    assert!(extract(&dir, plain).is_empty());
}

// --- end to end through the session ---------------------------------

struct NoEvents;
impl LayoutEvents for NoEvents {
    fn first_page_ready(&self, _: u64, _: u32) {}
    fn chapter_ready(&self, _: u64, _: u32, _: u32) {}
    fn chapter_failed(&self, _: u64, _: u32) {}
}

fn registry() -> Arc<FontRegistry> {
    let dir = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../assets/fonts"
    ));
    FontRegistry::load(dir).unwrap_or_else(|e| panic!("repo font set must load: {e}"))
}

fn settings(font: &str) -> LayoutSettings {
    LayoutSettings {
        reading_font: font.to_string(),
        ..LayoutSettings::default()
    }
}

/// Opens the fixture and returns the distinct font ids on page 0.
fn page0_font_ids(epub: &Path, pubfonts: &Path, reading_font: &str) -> Vec<u32> {
    let session = EngineSession::open(
        epub,
        registry(),
        Viewport {
            width: 390.0,
            height: 664.0,
        },
        settings(reading_font),
        None,
        0,
        Some(pubfonts),
        Arc::new(NoEvents),
    )
    .expect("session opens");
    let deadline = Instant::now() + Duration::from_secs(20);
    let page = loop {
        match session.page(0, 0) {
            Ok(page) => break page,
            Err(EngineError::NotReady) => {
                assert!(Instant::now() < deadline, "page 0 did not lay out");
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
    };
    let mut ids: Vec<u32> = page.glyph_runs.iter().map(|run| run.font_id).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// The flagship path: under the publisher reading font the embedded
/// face shapes the text (its id sits in the dynamic block and in
/// `fonts()`), while a non-publisher setting ignores the stack entirely
/// — and both registries still serve every id the pages reference.
#[test]
fn publisher_setting_shapes_with_the_embedded_face() {
    let dir = TempDir::new().unwrap();
    let epub = dir.path().join("book.epub");
    font_book(&font_bytes()).write(&epub);
    let pubfonts = dir.path().join("pubfonts");

    let ids = page0_font_ids(&epub, &pubfonts, "publisher");
    assert!(
        ids.iter().any(|&id| id >= FIRST_DYNAMIC_ID),
        "publisher setting must reach the embedded face, got {ids:?}"
    );

    let noto_ids = page0_font_ids(&epub, &pubfonts, "noto-serif");
    assert!(
        noto_ids.iter().all(|&id| id < FIRST_DYNAMIC_ID),
        "non-publisher settings must ignore font-family stacks, got {noto_ids:?}"
    );
}

/// The session's registry carries the publisher block, and its ids are
/// exactly the base block plus the appended faces.
#[test]
fn session_registry_extends_the_base_with_the_publisher_block() {
    let dir = TempDir::new().unwrap();
    let epub = dir.path().join("book.epub");
    font_book(&font_bytes()).write(&epub);
    let base = registry();
    let session = EngineSession::open(
        &epub,
        Arc::clone(&base),
        Viewport {
            width: 390.0,
            height: 664.0,
        },
        LayoutSettings::default(),
        None,
        0,
        Some(&dir.path().join("pubfonts")),
        Arc::new(NoEvents),
    )
    .expect("session opens");
    let entries = session.fonts().entries();
    assert_eq!(entries.len(), base.entries().len() + 1);
    let publisher = entries.last().expect("publisher entry");
    assert_eq!(publisher.id, base.next_free_id());
    assert!(publisher.file_path.contains("pubfonts"));

    // A fontless book keeps the base registry untouched.
    let plain = dir.path().join("plain.epub");
    EpubBuilder::new()
        .resource("ch01.xhtml", "application/xhtml+xml", CHAPTER.as_bytes())
        .spine(&["ch01.xhtml"])
        .write(&plain);
    let plain_session = EngineSession::open(
        &plain,
        Arc::clone(&base),
        Viewport {
            width: 390.0,
            height: 664.0,
        },
        LayoutSettings::default(),
        None,
        0,
        Some(&dir.path().join("pubfonts")),
        Arc::new(NoEvents),
    )
    .expect("session opens");
    assert_eq!(plain_session.fonts().entries().len(), base.entries().len());
}

/// Builds a spec-legal WOFF1 with every table stored uncompressed
/// (compLength == origLength) — enough to exercise the decode path
/// without a zlib encoder in the test tree.
fn wrap_woff1_stored(sfnt: &[u8]) -> Vec<u8> {
    let num_tables = u16::from_be_bytes([sfnt[4], sfnt[5]]) as usize;
    let dir_start = 12;
    let data_start = 44 + 20 * num_tables;
    let mut dir = Vec::new();
    let mut data = Vec::new();
    let mut total_sfnt = 12 + 16 * num_tables;
    for at in 0..num_tables {
        let entry = &sfnt[dir_start + 16 * at..dir_start + 16 * at + 16];
        let checksum = &entry[4..8];
        let offset = u32::from_be_bytes(entry[8..12].try_into().unwrap()) as usize;
        let length = u32::from_be_bytes(entry[12..16].try_into().unwrap()) as usize;
        dir.extend_from_slice(&entry[0..4]); // tag
        dir.extend_from_slice(&((data_start + data.len()) as u32).to_be_bytes());
        dir.extend_from_slice(&(length as u32).to_be_bytes()); // compLength
        dir.extend_from_slice(&(length as u32).to_be_bytes()); // origLength
        dir.extend_from_slice(checksum);
        data.extend_from_slice(&sfnt[offset..offset + length]);
        while data.len() % 4 != 0 {
            data.push(0);
        }
        total_sfnt += length.next_multiple_of(4);
    }
    let mut out = Vec::with_capacity(data_start + data.len());
    out.extend_from_slice(b"wOFF");
    out.extend_from_slice(&sfnt[0..4]); // flavor
    out.extend_from_slice(&((data_start + data.len()) as u32).to_be_bytes());
    out.extend_from_slice(&(num_tables as u16).to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // reserved
    out.extend_from_slice(&(total_sfnt as u32).to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // majorVersion
    out.extend_from_slice(&0u16.to_be_bytes()); // minorVersion
    out.extend_from_slice(&[0u8; 20]); // meta/priv offsets and lengths
    out.extend_from_slice(&dir);
    out.extend_from_slice(&data);
    out
}

/// C5: WOFF containers are bounded BEFORE decompression — a header
/// declaring a multi-gigabyte `totalSfntSize` (WOFF2 permits ~100×
/// expansion) is rejected without ever calling the decoder, as is a
/// compressed payload over the per-face cap.
#[test]
fn oversized_declared_sfnt_is_rejected_before_decompression() {
    // A minimal WOFF2 header: signature, flavor, length, numTables,
    // reserved, then totalSfntSize declaring ~2 GiB.
    let mut woff2 = Vec::new();
    woff2.extend_from_slice(b"wOF2");
    woff2.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // flavor
    woff2.extend_from_slice(&48u32.to_be_bytes()); // length
    woff2.extend_from_slice(&1u16.to_be_bytes()); // numTables
    woff2.extend_from_slice(&0u16.to_be_bytes()); // reserved
    woff2.extend_from_slice(&0x7FFF_FFFFu32.to_be_bytes()); // totalSfntSize
    woff2.resize(48, 0);

    let dir = TempDir::new().unwrap();
    let specs = extract(&dir, font_book(&woff2));
    assert!(
        specs.is_empty(),
        "a declared sfnt size over the per-face cap must be rejected"
    );
    // A truncated WOFF header (no declared size at all) is rejected too.
    let dir = TempDir::new().unwrap();
    let specs = extract(&dir, font_book(b"wOF2\x00"));
    assert!(specs.is_empty());
}

/// C6: every href of a consumed @font-face rule is claimed — the
/// losing `src` alternates must not re-register in pass 2 as duplicate
/// manifest-only faces.
#[test]
fn losing_src_alternates_do_not_duplicate_in_pass_two() {
    const ALT_CSS: &str = r#"
@font-face {
  font-family: "Pub Face";
  src: url(fonts/pub.woff2) format("woff2"), url(fonts/pub.ttf);
}
"#;
    // fonts/pub.woff2 is bogus (fails extraction) so fonts/pub.ttf wins;
    // both are manifest font items.
    let builder = EpubBuilder::new()
        .resource("ch01.xhtml", "application/xhtml+xml", CHAPTER.as_bytes())
        .resource("styles.css", "text/css", ALT_CSS.as_bytes())
        .resource("fonts/pub.woff2", "font/woff2", b"not a font")
        .resource("fonts/pub.ttf", "font/ttf", &font_bytes())
        .spine(&["ch01.xhtml"]);
    let dir = TempDir::new().unwrap();
    let specs = extract(&dir, builder);
    assert_eq!(
        specs.len(),
        1,
        "the rule's alternates must not resurface as pass-2 faces: {specs:?}"
    );
    assert_eq!(specs[0].family, "Pub Face");
}

/// C11: stale `<hash>.tmp` files from a crashed earlier extraction are
/// swept at the next extraction start.
#[test]
fn stale_tmp_files_are_swept() {
    let dir = TempDir::new().unwrap();
    let cache = dir.path().join("pubfonts");
    std::fs::create_dir_all(&cache).unwrap();
    let stale = cache.join("deadbeefdeadbeefdeadbeefdeadbeef.tmp");
    std::fs::write(&stale, b"half-written").unwrap();

    let epub = dir.path().join("book.epub");
    font_book(&font_bytes()).write(&epub);
    let package = read_package(&epub).expect("package parses");
    let specs = extract_publisher_fonts(&epub, &package, &cache);
    assert_eq!(specs.len(), 1);
    assert!(!stale.exists(), "stale tmp files must be swept");
}
