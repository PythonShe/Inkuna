use super::{Headers, parse_headers};

fn record0_with_exth(records: &[(u32, &[u8])]) -> Vec<u8> {
    let mobi_len = 228usize;
    let exth_len = 12
        + records
            .iter()
            .map(|(_, data)| 8 + data.len())
            .sum::<usize>();
    let mut bytes = vec![0; 16 + mobi_len + exth_len];
    bytes[0..2].copy_from_slice(&2u16.to_be_bytes());
    bytes[4..8].copy_from_slice(&1234u32.to_be_bytes());
    bytes[8..10].copy_from_slice(&2u16.to_be_bytes());
    bytes[10..12].copy_from_slice(&4096u16.to_be_bytes());
    bytes[16..20].copy_from_slice(b"MOBI");
    bytes[20..24].copy_from_slice(&(mobi_len as u32).to_be_bytes());
    bytes[28..32].copy_from_slice(&65001u32.to_be_bytes());
    bytes[36..40].copy_from_slice(&6u32.to_be_bytes());
    bytes[84..88].copy_from_slice(&300u32.to_be_bytes());
    bytes[88..92].copy_from_slice(&9u32.to_be_bytes());
    bytes[92..96].copy_from_slice(&0x0804u32.to_be_bytes());
    bytes[108..112].copy_from_slice(&8u32.to_be_bytes());
    bytes[112..116].copy_from_slice(&5u32.to_be_bytes());
    bytes[116..120].copy_from_slice(&3u32.to_be_bytes());
    bytes[128..132].copy_from_slice(&0x40u32.to_be_bytes());
    bytes[240..242].copy_from_slice(&0xaabbu16.to_be_bytes());
    bytes[242..244].copy_from_slice(&0x07u16.to_be_bytes());

    let mut cursor = 16 + mobi_len;
    bytes[cursor..cursor + 4].copy_from_slice(b"EXTH");
    bytes[cursor + 4..cursor + 8].copy_from_slice(&(exth_len as u32).to_be_bytes());
    bytes[cursor + 8..cursor + 12].copy_from_slice(&(records.len() as u32).to_be_bytes());
    cursor += 12;
    for (kind, data) in records {
        bytes[cursor..cursor + 4].copy_from_slice(&kind.to_be_bytes());
        bytes[cursor + 4..cursor + 8].copy_from_slice(&((8 + data.len()) as u32).to_be_bytes());
        bytes[cursor + 8..cursor + 8 + data.len()].copy_from_slice(data);
        cursor += 8 + data.len();
    }
    bytes
}

#[test]
fn parses_palmdoc_mobi_and_exth_fields() {
    let bytes = record0_with_exth(&[(100, b"Ada"), (121, &17u32.to_be_bytes())]);
    let Headers {
        palmdoc,
        mobi,
        exth,
    } = parse_headers(&bytes).unwrap();
    assert_eq!(palmdoc.compression, 2);
    assert_eq!(palmdoc.text_length, 1234);
    assert_eq!(palmdoc.record_count, 2);
    assert_eq!(palmdoc.record_size, 4096);
    assert_eq!(palmdoc.encryption_type, 0);
    assert_eq!(mobi.encoding, 65001);
    assert_eq!(mobi.file_version, 6);
    assert_eq!(mobi.fullname, Some((300, 9)));
    assert_eq!(mobi.locale, 0x0804);
    assert_eq!(mobi.first_image_index, Some(8));
    assert_eq!(mobi.huff_records, Some((5, 3)));
    assert_eq!(mobi.extra_data_flags, 7);
    assert_eq!(exth.len(), 2);
    assert_eq!(exth[0].kind, 100);
    assert_eq!(exth[0].data, b"Ada");
}

#[test]
fn rejects_truncated_headers_and_malformed_exth_lengths() {
    assert!(parse_headers(&[]).is_err());
    assert!(parse_headers(&vec![0; 15]).is_err());
    let valid = record0_with_exth(&[]);
    for length in [16, 19, 23, 111, 200, 243] {
        assert!(parse_headers(&valid[..length]).is_err(), "length {length}");
    }

    let mut short_mobi = record0_with_exth(&[]);
    short_mobi[20..24].copy_from_slice(&95u32.to_be_bytes());
    assert!(parse_headers(&short_mobi).is_err());

    let mut missing_magic = record0_with_exth(&[]);
    missing_magic[16..20].copy_from_slice(b"XXXX");
    assert!(parse_headers(&missing_magic).is_err());

    let mut bad_count = record0_with_exth(&[]);
    bad_count[252..256].copy_from_slice(&4097u32.to_be_bytes());
    assert!(parse_headers(&bad_count).is_err());

    let mut bad_record = record0_with_exth(&[(100, b"Ada")]);
    bad_record[260..264].copy_from_slice(&(1024u32 * 1024 + 1).to_be_bytes());
    assert!(parse_headers(&bad_record).is_err());
}

#[test]
fn refuses_unknown_compression_and_preserves_encryption_values() {
    let mut bytes = record0_with_exth(&[]);
    bytes[0..2].copy_from_slice(&99u16.to_be_bytes());
    assert!(parse_headers(&bytes).is_err());

    bytes[0..2].copy_from_slice(&1u16.to_be_bytes());
    bytes[12..14].copy_from_slice(&7u16.to_be_bytes());
    assert_eq!(parse_headers(&bytes).unwrap().palmdoc.encryption_type, 7);
}
