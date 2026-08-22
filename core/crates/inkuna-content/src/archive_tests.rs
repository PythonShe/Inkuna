use super::*;

/// A crafted entry that inflates past the per-entry cap is rejected on
/// the read instead of being materialized in full — without the cap
/// this allocates 65 MiB from a ~65 KB archive, and a real bomb scales
/// that to gigabytes.
#[test]
fn oversize_entry_hits_the_decompression_cap() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bomb.epub");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.start_file(
        "OEBPS/bomb.xhtml",
        zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated),
    )
    .unwrap();
    let chunk = vec![b' '; 1024 * 1024];
    let mut written = 0u64;
    while written <= MAX_XML_ENTRY_BYTES {
        zip.write_all(&chunk).unwrap();
        written += chunk.len() as u64;
    }
    zip.finish().unwrap();
    // The whole point: the archive on disk is tiny.
    assert!(std::fs::metadata(&path).unwrap().len() < 1024 * 1024);

    let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    assert!(matches!(
        read_entry(&mut archive, "OEBPS/bomb.xhtml"),
        Err(ContentError::InvalidPublication(_))
    ));
}

/// One archive, one entry sized between the two budgets: the spine
/// reader rejects it, the mandatory-XML reader accepts it — pinning that
/// the tight concurrent-read cap and the generous mandatory-part cap are
/// genuinely different bounds, not one constant with two names.
#[test]
fn spine_cap_is_tighter_than_the_mandatory_xml_cap() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("split.epub");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.start_file(
        "OEBPS/between.xhtml",
        zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated),
    )
    .unwrap();
    let size = MAX_SPINE_ENTRY_BYTES + 1024 * 1024; // 9 MiB: over spine, under XML
    let chunk = vec![b' '; 1024 * 1024];
    let mut written = 0u64;
    while written < size {
        zip.write_all(&chunk).unwrap();
        written += chunk.len() as u64;
    }
    zip.finish().unwrap();

    let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    assert!(matches!(
        read_spine_entry(&mut archive, "OEBPS/between.xhtml"),
        Err(ContentError::InvalidPublication(_))
    ));
    let xml = read_entry(&mut archive, "OEBPS/between.xhtml").unwrap();
    assert_eq!(xml.len() as u64, size);
}

/// An over-cap entry stays a cap rejection even when the capped prefix
/// ends mid-character. For CJK that is the normal case, not an exotic
/// one — every codepoint is multi-byte, so a byte-aligned cut almost
/// always splits one — and the shells route on the variant, so it must
/// not surface as an I/O decode error.
#[test]
fn oversize_cjk_entry_cut_mid_character_is_still_invalid_publication() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cjk-bomb.epub");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.start_file(
        "OEBPS/bomb.xhtml",
        zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated),
    )
    .unwrap();
    // Three bytes per character, and the cap + 1 boundary is not a
    // multiple of three, so the prefix necessarily ends inside one.
    let chunk = "書".repeat(1024 * 1024);
    let mut written = 0u64;
    while written <= MAX_XML_ENTRY_BYTES {
        zip.write_all(chunk.as_bytes()).unwrap();
        written += chunk.len() as u64;
    }
    zip.finish().unwrap();

    let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    match read_entry(&mut archive, "OEBPS/bomb.xhtml") {
        Err(ContentError::InvalidPublication(_)) => {}
        Err(other) => panic!("expected InvalidPublication, got {other:?}"),
        Ok(_) => panic!("expected the cap to reject the entry"),
    }
}

/// `read_resource` is the single bounded read for binary resources: a
/// crafted entry inflating past the per-entry budget is rejected as the
/// cap breach it is (`InvalidPublication`, the classification every
/// capped reader shares) instead of being materialized in full.
#[test]
fn read_resource_respects_entry_budget() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resource-bomb.epub");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("OEBPS/images/cover.png", deflated).unwrap();
    let chunk = vec![0u8; 1024 * 1024];
    let mut written = 0u64;
    while written <= MAX_RESOURCE_BYTES {
        zip.write_all(&chunk).unwrap();
        written += chunk.len() as u64;
    }
    zip.start_file("OEBPS/images/small.png", deflated).unwrap();
    zip.write_all(b"\x89PNG\r\n\x1a\nsmall").unwrap();
    zip.finish().unwrap();

    assert!(matches!(
        read_resource(&path, "OEBPS/images/cover.png"),
        Err(ContentError::InvalidPublication(_))
    ));
    // An in-budget sibling still reads back verbatim.
    assert_eq!(
        read_resource(&path, "OEBPS/images/small.png").unwrap(),
        b"\x89PNG\r\n\x1a\nsmall"
    );
}
