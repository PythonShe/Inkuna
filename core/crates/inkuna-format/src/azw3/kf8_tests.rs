use super::{assemble_files, parse_fdst, read, Fragment, Skeleton};
use crate::mobi::MobiBook;
use crate::test_support::{Kf8FileFixture, Kf8NcxFixture, MobiTestBuilder};
use crate::FormatError;

#[test]
fn parses_fdst_and_reassembles_skeletons_with_nontrivial_fragments() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("structure.azw3");
    let first_skeleton = b"<html><body><p>AC</p></body></html>";
    let second_skeleton = b"<html><body><h1>Two</h1><p>XZ</p></body></html>";
    let first_insert = first_skeleton
        .iter()
        .position(|byte| *byte == b'C')
        .unwrap();
    let second_insert = second_skeleton
        .iter()
        .position(|byte| *byte == b'Z')
        .unwrap();
    MobiTestBuilder::new(8)
        .kf8_files(vec![
            Kf8FileFixture::new(first_skeleton).fragment(first_insert, b"B"),
            Kf8FileFixture::new(second_skeleton).fragment(second_insert, "月".as_bytes()),
        ])
        .css_flow(b"p { color: black; }")
        .ncx(vec![
            Kf8NcxFixture::new("第一部", 0, 1),
            Kf8NcxFixture::new("第二章", 1, 2),
        ])
        .write(&path);

    let book = MobiBook::open(&path).unwrap();
    let content = read(&book).unwrap();

    assert_eq!(content.files.len(), 2);
    assert_eq!(
        content.files[0].bytes,
        b"<html><body><p>ABC</p></body></html>"
    );
    assert_eq!(
        content.files[1].bytes,
        "<html><body><h1>Two</h1><p>X月Z</p></body></html>".as_bytes()
    );
    assert_eq!(content.flows.len(), 2);
    assert_eq!(content.flows[1], b"p { color: black; }");
    assert_eq!(content.fragment_file(0), Some(0));
    assert_eq!(content.fragment_file(1), Some(1));
    assert_eq!(content.files[0].anchors[0].fid, 0);
    assert_eq!(content.files[1].anchors[0].fid, 1);
    let toc = content.toc.as_ref().unwrap();
    assert_eq!(toc[0].label, "第一部");
    assert_eq!(toc[0].file, 0);
    assert_eq!(toc[0].depth, 1);
    assert_eq!(toc[1].label, "第二章");
    assert_eq!(toc[1].file, 1);
    assert_eq!(toc[1].depth, 2);
}

#[test]
fn resolves_ncx_titles_through_the_cncx_string_pool() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cncx.azw3");
    MobiTestBuilder::new(8)
        .kf8_files(vec![Kf8FileFixture::new(
            b"<html><body><p>A</p></body></html>",
        )])
        .ncx(vec![Kf8NcxFixture::new("第一部", 0, 1)])
        .write(&path);

    let book = MobiBook::open(&path).unwrap();
    let content = read(&book).unwrap();

    // The fixture writes the numeric ordinal b"0000" into the raw NCX entry
    // label; the title must be resolved through the CNCX record via tag 3.
    let toc = content.toc.as_ref().unwrap();
    assert_eq!(toc[0].label, "第一部");
    assert_eq!(toc[0].file, 0);
    assert_eq!(toc[0].depth, 1);
}

#[test]
fn oversized_cncx_pools_fall_back_to_raw_ncx_labels() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cncx-flood.azw3");
    // A title larger than MAX_CNCX_BYTES makes the CNCX record blow the
    // pool budget; the pool must be dropped without being read, degrading
    // the TOC to the raw ordinal labels instead of failing the import.
    let flood = "a".repeat((1 << 20) + 1);
    MobiTestBuilder::new(8)
        .kf8_files(vec![Kf8FileFixture::new(
            b"<html><body><p>A</p></body></html>",
        )])
        .ncx(vec![Kf8NcxFixture::new(&flood, 0, 1)])
        .write(&path);

    let book = MobiBook::open(&path).unwrap();
    let content = read(&book).unwrap();

    let toc = content.toc.as_ref().unwrap();
    assert_eq!(toc[0].label, "0000");
}

#[test]
fn fdst_pairs_are_bounded_and_fully_checked() {
    let mut valid = Vec::from(b"FDST".as_slice());
    valid.extend_from_slice(&28u32.to_be_bytes());
    valid.extend_from_slice(&2u32.to_be_bytes());
    valid.extend_from_slice(&0u32.to_be_bytes());
    valid.extend_from_slice(&10u32.to_be_bytes());
    valid.extend_from_slice(&10u32.to_be_bytes());
    valid.extend_from_slice(&20u32.to_be_bytes());
    assert_eq!(parse_fdst(&valid, 20).unwrap(), [(0, 10), (10, 20)]);

    for length in 0..valid.len() {
        let result = std::panic::catch_unwind(|| parse_fdst(&valid[..length], 20));
        assert!(result.is_ok(), "length {length} panicked");
        assert!(result.unwrap().is_err(), "length {length}");
    }
    let mut out_of_bounds = valid.clone();
    out_of_bounds[24..28].copy_from_slice(&21u32.to_be_bytes());
    assert!(parse_fdst(&out_of_bounds, 20).is_err());
}

#[test]
fn reassembly_meters_total_assembled_bytes_against_the_text_limit() {
    // One skeleton whose fragments all repeat the same large flow-0 range:
    // 4 x 24 MiB + the skeleton crosses the 96 MiB assembled-text ceiling.
    let fragment_length = 24 * 1024 * 1024;
    let flow0 = vec![b'x'; 2 + fragment_length];
    let skeletons = vec![Skeleton {
        fragment_count: 4,
        start: 0,
        length: 2,
    }];
    let fragments = (0..4u32)
        .map(|fid| Fragment {
            fid,
            insert: 1,
            file: 0,
            sequence: fid as usize,
            start: 2,
            length: fragment_length,
        })
        .collect::<Vec<_>>();

    assert!(matches!(
        assemble_files(&flow0, &skeletons, &fragments),
        Err(FormatError::InvalidPublication(_))
    ));
}

#[test]
fn reassembly_rejects_fragment_count_and_offset_mismatches() {
    let skeletons = vec![Skeleton {
        fragment_count: 2,
        start: 0,
        length: 2,
    }];
    let fragments = vec![Fragment {
        fid: 0,
        insert: 3,
        file: 0,
        sequence: 0,
        start: 2,
        length: 1,
    }];

    assert!(matches!(
        assemble_files(b"abc", &skeletons, &fragments),
        Err(FormatError::InvalidPublication(_))
    ));
}
