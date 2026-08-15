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
        Err(CoreError::InvalidPublication(_))
    ));
}
