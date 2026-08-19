use super::parse_records;
use crate::test_support::{build_indx_records, IndxEntryFixture};
use crate::CoreError;

fn fixture() -> Vec<Vec<u8>> {
    build_indx_records(
        &[(1, 1, 0x03, 0), (6, 2, 0x04, 0)],
        &[
            IndxEntryFixture::new(b"chapter-one")
                .tag(1, &[1, 130])
                .tag(6, &[4_096, 257]),
            IndxEntryFixture::new("第二章".as_bytes())
                .tag(1, &[7])
                .tag(6, &[8_192, 511]),
        ],
    )
}

#[test]
fn parses_real_indx_tagx_idxt_layout_and_forward_varints() {
    let records = fixture();
    assert_eq!(&records[0][records[0].len() - 4..], &[0, 0, 0, 1]);

    let index = parse_records(&records).unwrap();

    assert_eq!(index.entries.len(), 2);
    assert_eq!(index.entries[0].label, b"chapter-one");
    assert_eq!(index.entries[0].values(1), Some(&[1, 130][..]));
    assert_eq!(index.entries[0].values(6), Some(&[4_096, 257][..]));
    assert_eq!(index.entries[1].label, "第二章".as_bytes());
    assert_eq!(index.entries[1].values(1), Some(&[7][..]));
    assert_eq!(index.entries[1].values(6), Some(&[8_192, 511][..]));
}

#[test]
fn parses_tagx_entries_across_multiple_control_bytes() {
    let records = build_indx_records(
        &[
            (1, 1, 0x01, 0),
            (0, 0, 0, 1),
            (3, 1, 0x01, 0),
            (6, 2, 0x02, 0),
        ],
        &[IndxEntryFixture::new(b"17")
            .tag(1, &[9])
            .tag(3, &[2])
            .tag(6, &[300, 40])],
    );

    let index = parse_records(&records).unwrap();

    assert_eq!(index.entries[0].values(1), Some(&[9][..]));
    assert_eq!(index.entries[0].values(3), Some(&[2][..]));
    assert_eq!(index.entries[0].values(6), Some(&[300, 40][..]));
}

#[test]
fn decodes_the_real_frag_tag_table_with_single_bit_masks() {
    let records = build_indx_records(
        &[
            (2, 1, 0x01, 0),
            (3, 1, 0x02, 0),
            (4, 1, 0x04, 0),
            (6, 2, 0x08, 0),
        ],
        &[IndxEntryFixture::new(b"0000")
            .tag(2, &[5])
            .tag(3, &[1])
            .tag(4, &[0])
            .tag(6, &[1_024, 96])],
    );

    let index = parse_records(&records).unwrap();

    assert_eq!(index.entries[0].label, b"0000");
    assert_eq!(index.entries[0].values(2), Some(&[5][..]));
    assert_eq!(index.entries[0].values(3), Some(&[1][..]));
    assert_eq!(index.entries[0].values(4), Some(&[0][..]));
    assert_eq!(index.entries[0].values(6), Some(&[1_024, 96][..]));
}

#[test]
fn round_trips_a_multi_bit_all_mask_byte_length_budget() {
    let mut values = (0..15).collect::<Vec<u32>>();
    values.push(300);
    let records = build_indx_records(
        &[(1, 1, 0x0f, 0)],
        &[IndxEntryFixture::new(b"budget").tag(1, &values)],
    );

    let index = parse_records(&records).unwrap();

    assert_eq!(index.entries[0].values(1), Some(values.as_slice()));
}

#[test]
fn rejects_truncated_indx_records_without_panicking() {
    let records = fixture();
    for record_index in 0..records.len() {
        for length in 0..records[record_index].len() {
            let mut truncated = records.clone();
            truncated[record_index].truncate(length);
            let result = std::panic::catch_unwind(|| parse_records(&truncated));
            assert!(
                result.is_ok(),
                "record {record_index}, length {length} panicked"
            );
            assert!(
                result.unwrap().is_err(),
                "record {record_index}, length {length}"
            );
        }
    }
}

#[test]
fn rejects_hostile_counts_offsets_and_tag_expansion() {
    let mut excessive_total = fixture();
    excessive_total[0][36..40].copy_from_slice(&65_537u32.to_be_bytes());
    assert!(matches!(
        parse_records(&excessive_total),
        Err(CoreError::InvalidPublication(_))
    ));

    let mut bad_idxt = fixture();
    bad_idxt[1][20..24].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(parse_records(&bad_idxt).is_err());

    let mut too_many_tags = build_indx_records(
        &[(1, 1, 0x7e, 0)],
        &[IndxEntryFixture::new(b"bomb").tag(1, &vec![1; 65])],
    );
    // The all-mask value announces a byte-length budget; the builder encoded
    // 65 single-byte varint values, which overruns the 64-tag ceiling.
    assert!(parse_records(&too_many_tags).is_err());

    let idxt = u32::from_be_bytes(too_many_tags[1][20..24].try_into().unwrap()) as usize;
    too_many_tags[1][idxt..idxt + 4].copy_from_slice(b"NOPE");
    assert!(parse_records(&too_many_tags).is_err());
}
