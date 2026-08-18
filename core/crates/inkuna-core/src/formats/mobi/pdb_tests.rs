use super::{check_container_size, PalmDatabase};

fn pdb_bytes(offsets: &[u32], records: &[&[u8]]) -> Vec<u8> {
    let mut bytes = vec![0; 78];
    bytes[..9].copy_from_slice(b"Test Book");
    bytes[60..68].copy_from_slice(b"BOOKMOBI");
    bytes[76..78].copy_from_slice(&(offsets.len() as u16).to_be_bytes());
    for offset in offsets {
        bytes.extend_from_slice(&offset.to_be_bytes());
        bytes.extend_from_slice(&[0; 4]);
    }
    while bytes.len() < offsets.first().copied().unwrap_or(78) as usize {
        bytes.push(0);
    }
    for record in records {
        bytes.extend_from_slice(record);
    }
    bytes
}

#[test]
fn parses_name_and_record_ranges() {
    let bytes = pdb_bytes(&[96, 99], &[b"one", b"second"]);
    let database = PalmDatabase::parse(bytes).unwrap();
    assert_eq!(database.name(), b"Test Book");
    assert_eq!(database.record_count(), 2);
    assert_eq!(database.record(0).unwrap(), b"one");
    assert_eq!(database.record(1).unwrap(), b"second");
}

#[test]
fn rejects_record_offsets_outside_the_file_or_out_of_order() {
    let past_eof = pdb_bytes(&[96, 999], &[b"one"]);
    assert!(PalmDatabase::parse(past_eof).is_err());

    let descending = pdb_bytes(&[100, 99], &[b"one"]);
    assert!(PalmDatabase::parse(descending).is_err());

    let inside_table = pdb_bytes(&[79], &[b"one"]);
    assert!(PalmDatabase::parse(inside_table).is_err());

    let at_eof = pdb_bytes(&[86], &[]);
    assert!(PalmDatabase::parse(at_eof).is_err());
}

#[test]
fn rejects_truncated_headers_and_excessive_record_counts() {
    assert!(PalmDatabase::parse(vec![0; 77]).is_err());

    let mut bytes = vec![0; 78];
    bytes[76..78].copy_from_slice(&16_385u16.to_be_bytes());
    assert!(PalmDatabase::parse(bytes).is_err());
}

#[test]
fn rejects_an_oversized_container_before_allocating_it() {
    assert!(check_container_size(1024 * 1024 * 1024).is_ok());
    assert!(check_container_size(1024 * 1024 * 1024 + 1).is_err());
}

#[test]
fn open_reads_record_payloads_lazily_from_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lazy.mobi");
    std::fs::write(&path, pdb_bytes(&[96, 99], &[b"one", b"second"])).unwrap();
    let database = PalmDatabase::open(&path).unwrap();

    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(94)
        .unwrap();
    assert!(database.record(0).is_err());
}
