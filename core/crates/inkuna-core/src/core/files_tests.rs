use super::*;

/// A stream is bounded by the ceiling, not by the length its provider
/// claims — the whole point of the guard, since a descriptor handed over
/// by a file provider can keep producing bytes forever. The ceiling is
/// injected so the refusal is reached without moving 2 GiB.
#[test]
fn a_stream_past_the_ceiling_is_refused_before_it_fills_the_disk() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("endless.epub.tmp");
    // 4 MiB of an endless stream against a 1 MiB ceiling: more than one
    // read buffer past the limit, so the loop must stop on its own rather
    // than at end-of-stream.
    let mut endless = std::io::repeat(0xAB).take(4 * 1024 * 1024);

    let err = stream_and_hash_capped(&mut endless, &dest, 1024 * 1024).unwrap_err();
    match err {
        CoreError::FileTooLarge(limit) => assert_eq!(limit, 1024 * 1024),
        other => panic!("expected FileTooLarge, got {other:?}"),
    }
    // What reached the disk stayed under the ceiling. The partial file is
    // the caller's to sweep, which the import pipeline does.
    assert!(std::fs::metadata(&dest).unwrap().len() <= 1024 * 1024);
}

/// The ceiling is inclusive: a file of exactly the maximum size is a
/// legitimate import, not an attack.
#[test]
fn a_stream_exactly_at_the_ceiling_still_imports() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("brimful.epub.tmp");
    let bytes = vec![0x7Fu8; 512 * 1024];

    let hash = stream_and_hash_capped(&mut bytes.as_slice(), &dest, bytes.len() as u64).unwrap();
    assert_eq!(hash, blake3::hash(&bytes).to_hex().to_string());
    assert_eq!(std::fs::read(&dest).unwrap(), bytes);
}

/// The path route inherits the same ceiling, since it delegates: an
/// ordinary book copies and hashes under the production constant.
#[test]
fn the_path_route_copies_and_hashes_under_the_production_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("本.epub");
    let dest = dir.path().join("本.epub.tmp");
    let bytes = "月光書房".repeat(4096).into_bytes();
    std::fs::write(&src, &bytes).unwrap();

    let hash = copy_and_hash(&src, &dest).unwrap();
    assert_eq!(hash, blake3::hash(&bytes).to_hex().to_string());
    assert_eq!(std::fs::read(&dest).unwrap(), bytes);
}
