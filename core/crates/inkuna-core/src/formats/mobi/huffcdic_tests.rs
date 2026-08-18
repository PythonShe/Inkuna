use super::HuffCdic;

fn huff_record() -> Vec<u8> {
    let mut record = vec![0; 1304];
    record[..4].copy_from_slice(b"HUFF");
    record[4..8].copy_from_slice(&24u32.to_be_bytes());
    record[8..12].copy_from_slice(&24u32.to_be_bytes());
    record[12..16].copy_from_slice(&1048u32.to_be_bytes());
    // Every byte is a terminal 8-bit code. The dictionary index is
    // 255-code, so FF selects phrase 0 and FE selects phrase 1.
    for index in 0..256 {
        let offset = 24 + index * 4;
        record[offset..offset + 4].copy_from_slice(&0x0000_ff88u32.to_be_bytes());
    }
    record
}

fn cdic_record(nested: bool) -> Vec<u8> {
    let mut record = vec![0; 26];
    record[..4].copy_from_slice(b"CDIC");
    record[4..8].copy_from_slice(&16u32.to_be_bytes());
    record[8..12].copy_from_slice(&2u32.to_be_bytes());
    record[12..16].copy_from_slice(&1u32.to_be_bytes());
    record[16..18].copy_from_slice(&4u16.to_be_bytes());
    record[18..20].copy_from_slice(&7u16.to_be_bytes());
    record[20..22].copy_from_slice(&0x8001u16.to_be_bytes());
    record[22] = b'X';
    let second_length: u16 = if nested { 1 } else { 0x8001 };
    record[23..25].copy_from_slice(&second_length.to_be_bytes());
    record[25] = if nested { 0xff } else { b'Y' };
    record
}

fn three_bit_records() -> (Vec<u8>, Vec<u8>) {
    let mut huff = huff_record();
    for index in 0..256 {
        let offset = 24 + index * 4;
        huff[offset..offset + 4].copy_from_slice(&0x0000_0783u32.to_be_bytes());
    }
    let mut cdic = vec![0; 56];
    cdic[..4].copy_from_slice(b"CDIC");
    cdic[4..8].copy_from_slice(&16u32.to_be_bytes());
    cdic[8..12].copy_from_slice(&8u32.to_be_bytes());
    cdic[12..16].copy_from_slice(&3u32.to_be_bytes());
    for index in 0..8 {
        let offset = 16 + index * 2;
        cdic[offset..offset + 2].copy_from_slice(&(16u16 + index as u16 * 3).to_be_bytes());
        let phrase = 32 + index * 3;
        cdic[phrase..phrase + 2].copy_from_slice(&0x8001u16.to_be_bytes());
        cdic[phrase + 2] = b'A' + index as u8;
    }
    (huff, cdic)
}

#[test]
fn decodes_terminal_and_nested_dictionary_phrases() {
    let huff = huff_record();
    let cdic = cdic_record(true);
    let decoder = HuffCdic::parse(&huff, &[cdic.as_slice()]).unwrap();
    assert_eq!(decoder.decompress(&[0xff, 0xfe], 64).unwrap(), b"XX");
}

#[test]
fn enforces_output_and_recursive_expansion_budgets() {
    let huff = huff_record();
    let terminal = cdic_record(false);
    let decoder = HuffCdic::parse(&huff, &[terminal.as_slice()]).unwrap();
    assert!(decoder.decompress(&[0xff, 0xfe], 1).is_err());

    let mut loop_dic = vec![0; 21];
    loop_dic[..4].copy_from_slice(b"CDIC");
    loop_dic[4..8].copy_from_slice(&16u32.to_be_bytes());
    loop_dic[8..12].copy_from_slice(&1u32.to_be_bytes());
    loop_dic[12..16].copy_from_slice(&0u32.to_be_bytes());
    loop_dic[16..18].copy_from_slice(&2u16.to_be_bytes());
    loop_dic[18..20].copy_from_slice(&1u16.to_be_bytes());
    loop_dic[20] = 0xff;
    let looping = HuffCdic::parse(&huff, &[loop_dic.as_slice()]).unwrap();
    let error = looping.decompress(&[0xff], 64).unwrap_err();
    assert!(error.to_string().contains("depth"));
}

#[test]
fn ignores_incomplete_padding_bits_after_the_last_code() {
    let (huff, cdic) = three_bit_records();
    let decoder = HuffCdic::parse(&huff, &[cdic.as_slice()]).unwrap();
    assert_eq!(decoder.decompress(&[0b1111_1000], 64).unwrap(), b"AB");
}

#[test]
fn rejects_truncated_or_inconsistent_tables() {
    assert!(HuffCdic::parse(b"HUFF", &[]).is_err());
    assert!(HuffCdic::parse(&huff_record(), &[b"CDIC".as_slice()]).is_err());

    let mut bad = cdic_record(false);
    bad[12..16].copy_from_slice(&31u32.to_be_bytes());
    assert!(HuffCdic::parse(&huff_record(), &[bad.as_slice()]).is_err());

    let mut impossible_terminal = huff_record();
    impossible_terminal[24..28].copy_from_slice(&0x0000_ff89u32.to_be_bytes());
    assert!(HuffCdic::parse(&impossible_terminal, &[cdic_record(false).as_slice()]).is_err());

    let mut unused_large_minimum = huff_record();
    unused_large_minimum[1048..1052].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(HuffCdic::parse(&unused_large_minimum, &[cdic_record(false).as_slice()]).is_ok());
}
