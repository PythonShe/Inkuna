use std::io::Cursor;

use image::{DynamicImage, ImageFormat, ImageReader, RgbaImage};

use super::{normalize_cover, normalized};
use crate::formats::epub::Cover;
use crate::test_support::{imported, write_epub};
use crate::Library;

/// A gradient PNG — compressible but photographic enough that lossy WebP
/// beats it on size.
fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut image = RgbaImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255]);
    }
    let mut bytes = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();
    bytes
}

fn dimensions(bytes: &[u8]) -> (u32, u32) {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .unwrap()
        .into_dimensions()
        .unwrap()
}

#[test]
fn oversized_cover_is_downscaled_to_webp() {
    let cover = normalize_cover(Cover {
        bytes: png_bytes(1200, 1800),
        extension: "png".into(),
    });
    assert_eq!(cover.extension, "webp");
    assert_eq!(dimensions(&cover.bytes), (600, 900));
}

#[test]
fn small_cover_reencodes_to_webp_at_original_size() {
    let cover = normalize_cover(Cover {
        bytes: png_bytes(300, 450),
        extension: "png".into(),
    });
    assert_eq!(cover.extension, "webp");
    assert_eq!(dimensions(&cover.bytes), (300, 450));
}

#[test]
fn undecodable_bytes_pass_through() {
    let bytes = b"\x89PNG\r\n\x1a\nfake png bytes".to_vec();
    let cover = normalize_cover(Cover {
        bytes: bytes.clone(),
        extension: "png".into(),
    });
    assert_eq!(cover.bytes, bytes);
    assert_eq!(cover.extension, "png");
}

#[test]
fn svg_passes_through() {
    let bytes = br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#.to_vec();
    let cover = normalize_cover(Cover {
        bytes: bytes.clone(),
        extension: "svg".into(),
    });
    assert_eq!(cover.bytes, bytes);
    assert_eq!(cover.extension, "svg");
}

#[test]
fn normalized_output_is_a_fixed_point() {
    let first = normalize_cover(Cover {
        bytes: png_bytes(1200, 1800),
        extension: "png".into(),
    });
    assert_eq!(first.extension, "webp");
    // Running it again must not stack another generation of loss.
    assert!(normalized(&first.bytes, &first.extension).is_none());
}

#[test]
fn dimension_bomb_passes_through_undecoded() {
    // A bare BMP header declaring 30000×30000 (900 megapixels) with no
    // pixel data behind it: the pre-decode cap must refuse it from the
    // header alone rather than letting a decoder try.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&54u32.to_le_bytes()); // declared file size
    bytes.extend_from_slice(&0u32.to_le_bytes()); // reserved
    bytes.extend_from_slice(&54u32.to_le_bytes()); // pixel data offset
    bytes.extend_from_slice(&40u32.to_le_bytes()); // BITMAPINFOHEADER
    bytes.extend_from_slice(&30_000i32.to_le_bytes()); // width
    bytes.extend_from_slice(&30_000i32.to_le_bytes()); // height
    bytes.extend_from_slice(&1u16.to_le_bytes()); // planes
    bytes.extend_from_slice(&24u16.to_le_bytes()); // bits per pixel
    bytes.extend_from_slice(&[0u8; 24]); // compression through palette
                                         // The header must parse and must declare more pixels than the cap
                                         // admits — otherwise the pass-through below could come from a mere
                                         // decode failure and the test would pass with the cap deleted.
    assert_eq!(dimensions(&bytes), (30_000, 30_000));
    let cover = normalize_cover(Cover {
        bytes: bytes.clone(),
        extension: "bmp".into(),
    });
    assert_eq!(cover.bytes, bytes);
    assert_eq!(cover.extension, "bmp");
}

#[test]
fn optimize_covers_reencodes_legacy_rows_once() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let library = Library::open(&data_dir).unwrap();

    let book = dir.path().join("book.epub");
    write_epub(&book, "书名", "作者", "zh");
    let publication = imported(library.import(book.to_str().unwrap()).unwrap());

    // Plant a pre-normalization cover: a full-resolution PNG on disk with
    // the row pointing at it, exactly what an older core left behind.
    let legacy_rel = format!("covers/{}.png", publication.id);
    std::fs::write(data_dir.join(&legacy_rel), png_bytes(1200, 1800)).unwrap();
    library
        .writer
        .lock()
        .unwrap()
        .execute(
            "UPDATE publications SET cover_path = ?1 WHERE id = ?2",
            rusqlite::params![legacy_rel, publication.id],
        )
        .unwrap();

    assert_eq!(library.optimize_covers().unwrap(), 1);

    let refreshed = library.publication(&publication.id).unwrap();
    let webp_rel = format!("covers/{}.webp", publication.id);
    assert_eq!(refreshed.cover_path.as_deref(), Some(webp_rel.as_str()));
    assert!(!data_dir.join(&legacy_rel).exists());
    let stored = std::fs::read(data_dir.join(&webp_rel)).unwrap();
    assert_eq!(dimensions(&stored), (600, 900));

    // Idempotent: the normalized cover is a fixed point.
    assert_eq!(library.optimize_covers().unwrap(), 0);
}
