//! KF8 FDST flow splitting and SKEL/FRAG reassembly.

use super::book::MobiBook;
use super::indx::{self, Index, IndexEntry};
use crate::CoreError;

const MAX_FLOWS: usize = 65_536;
const MAX_INDEX_RECORDS: usize = 65_536;

pub(super) struct Kf8Content {
    pub(super) files: Vec<AssembledFile>,
    pub(super) flows: Vec<Vec<u8>>,
    pub(super) toc: Option<Vec<Kf8TocEntry>>,
    fragment_files: Vec<Option<usize>>,
}

impl Kf8Content {
    pub(super) fn fragment_file(&self, fid: u32) -> Option<usize> {
        self.fragment_files.get(fid as usize).copied().flatten()
    }
}

pub(super) struct AssembledFile {
    pub(super) bytes: Vec<u8>,
    pub(super) anchors: Vec<FragmentAnchor>,
}

pub(super) struct FragmentAnchor {
    pub(super) fid: u32,
    pub(super) offset: usize,
}

pub(super) struct Kf8TocEntry {
    pub(super) label: String,
    pub(super) file: usize,
    pub(super) depth: usize,
}

pub(super) struct Skeleton {
    pub(super) fragment_count: usize,
    pub(super) start: usize,
    pub(super) length: usize,
}

pub(super) struct Fragment {
    pub(super) fid: u32,
    pub(super) insert: usize,
    pub(super) file: usize,
    pub(super) sequence: usize,
    pub(super) start: usize,
    pub(super) length: usize,
}

pub(super) fn read(book: &MobiBook) -> Result<Kf8Content, CoreError> {
    let pointers = book.kf8_pointers()?;
    let raw = book.text()?;
    let fdst = book.relative_record(pointers.fdst)?;
    let pairs = parse_fdst(fdst, raw.len())?;
    if pairs.len() != pointers.flow_count as usize {
        return Err(invalid("FDST flow count does not match the MOBI header"));
    }
    let flows = pairs
        .iter()
        .map(|(start, end)| raw[*start..*end].to_vec())
        .collect::<Vec<_>>();
    let flow0 = flows
        .first()
        .ok_or_else(|| invalid("KF8 FDST has no primary flow"))?;

    let skeleton_index = load_index(book, pointers.skeleton_index)?;
    let fragment_index = load_index(book, pointers.fragment_index)?;
    let skeletons = parse_skeletons(&skeleton_index)?;
    let fragments = parse_fragments(&fragment_index)?;
    let files = assemble_files(flow0, &skeletons, &fragments)?;
    let mut fragment_files = vec![None; fragments.len()];
    for fragment in &fragments {
        let slot = fragment_files
            .get_mut(fragment.fid as usize)
            .ok_or_else(|| invalid("KF8 fragment id is out of range"))?;
        *slot = Some(fragment.file);
    }
    let toc = pointers
        .ncx_index
        .or(pointers.guide_index)
        .and_then(|record| parse_toc(book, record, &skeletons, &fragments).ok())
        .filter(|entries| !entries.is_empty());
    Ok(Kf8Content {
        files,
        flows,
        toc,
        fragment_files,
    })
}

pub(super) fn parse_fdst(
    record: &[u8],
    raw_length: usize,
) -> Result<Vec<(usize, usize)>, CoreError> {
    if record.get(..4) != Some(b"FDST") {
        return Err(invalid("KF8 FDST record has the wrong signature"));
    }
    let declared_length = read_u32(record, 4)? as usize;
    let count = read_u32(record, 8)? as usize;
    if count == 0 || count > MAX_FLOWS {
        return Err(invalid("KF8 FDST flow count is invalid"));
    }
    let required = count
        .checked_mul(8)
        .and_then(|bytes| bytes.checked_add(12))
        .ok_or_else(|| invalid("KF8 FDST table length overflow"))?;
    if declared_length < required || declared_length > record.len() {
        return Err(invalid("truncated KF8 FDST table"));
    }
    let mut pairs = Vec::with_capacity(count);
    let mut previous_end = 0usize;
    for index in 0..count {
        let offset = 12 + index * 8;
        let start = read_u32(record, offset)? as usize;
        let end = read_u32(record, offset + 4)? as usize;
        if start != previous_end || start > end || end > raw_length {
            return Err(invalid("KF8 FDST flow boundaries are invalid"));
        }
        pairs.push((start, end));
        previous_end = end;
    }
    Ok(pairs)
}

fn load_index(book: &MobiBook, first: u32) -> Result<Index, CoreError> {
    let meta = book.relative_record(first)?;
    let record_count = read_u32(meta, 24)? as usize;
    if record_count == 0 || record_count > MAX_INDEX_RECORDS {
        return Err(invalid("KF8 INDX data record count is invalid"));
    }
    let mut records = Vec::with_capacity(record_count + 1);
    records.push(meta);
    for relative in 1..=record_count {
        let index = first
            .checked_add(relative as u32)
            .ok_or_else(|| invalid("KF8 INDX record index overflow"))?;
        records.push(book.relative_record(index)?);
    }
    indx::parse(&records)
}

fn parse_skeletons(index: &Index) -> Result<Vec<Skeleton>, CoreError> {
    index
        .entries
        .iter()
        .map(|entry| {
            let fragment_count = one(entry, 1, "SKEL fragment count")? as usize;
            let range = pair(entry, 6, "SKEL byte range")?;
            Ok(Skeleton {
                fragment_count,
                start: range.0 as usize,
                length: range.1 as usize,
            })
        })
        .collect()
}

fn parse_fragments(index: &Index) -> Result<Vec<Fragment>, CoreError> {
    index
        .entries
        .iter()
        .enumerate()
        .map(|(fid, entry)| {
            let label = std::str::from_utf8(&entry.label)
                .map_err(|_| invalid("FRAG insert offset is not UTF-8"))?;
            let insert = label
                .parse::<usize>()
                .map_err(|_| invalid("FRAG insert offset is not decimal"))?;
            let range = pair(entry, 6, "FRAG byte range")?;
            Ok(Fragment {
                fid: fid as u32,
                insert,
                file: one(entry, 3, "FRAG file number")? as usize,
                sequence: one(entry, 4, "FRAG sequence")? as usize,
                start: range.0 as usize,
                length: range.1 as usize,
            })
        })
        .collect()
}

pub(super) fn assemble_files(
    flow0: &[u8],
    skeletons: &[Skeleton],
    fragments: &[Fragment],
) -> Result<Vec<AssembledFile>, CoreError> {
    let declared_fragments = skeletons.iter().try_fold(0usize, |total, skeleton| {
        total
            .checked_add(skeleton.fragment_count)
            .ok_or_else(|| invalid("SKEL fragment count overflow"))
    })?;
    if declared_fragments != fragments.len() {
        return Err(invalid("SKEL fragment counts do not match FRAG entries"));
    }
    let mut files = Vec::with_capacity(skeletons.len());
    for (file_number, skeleton) in skeletons.iter().enumerate() {
        let skeleton_bytes = slice(flow0, skeleton.start, skeleton.length, "SKEL")?;
        let mut owned = fragments
            .iter()
            .filter(|fragment| fragment.file == file_number)
            .collect::<Vec<_>>();
        if owned.len() != skeleton.fragment_count {
            return Err(invalid("SKEL per-file fragment count does not match FRAG"));
        }
        owned.sort_by_key(|fragment| fragment.sequence);
        if owned
            .iter()
            .enumerate()
            .any(|(sequence, fragment)| fragment.sequence != sequence)
        {
            return Err(invalid("FRAG sequence numbers are not contiguous"));
        }
        let mut bytes = Vec::new();
        let mut anchors = Vec::with_capacity(owned.len());
        let mut cursor = 0usize;
        for fragment in owned {
            if fragment.insert < cursor || fragment.insert > skeleton_bytes.len() {
                return Err(invalid("FRAG insert offset is outside its skeleton"));
            }
            bytes.extend_from_slice(&skeleton_bytes[cursor..fragment.insert]);
            anchors.push(FragmentAnchor {
                fid: fragment.fid,
                offset: bytes.len(),
            });
            bytes.extend_from_slice(slice(flow0, fragment.start, fragment.length, "FRAG")?);
            cursor = fragment.insert;
        }
        bytes.extend_from_slice(&skeleton_bytes[cursor..]);
        files.push(AssembledFile { bytes, anchors });
    }
    if fragments
        .iter()
        .any(|fragment| fragment.file >= skeletons.len())
    {
        return Err(invalid("FRAG owns a nonexistent skeleton file"));
    }
    Ok(files)
}

fn parse_toc(
    book: &MobiBook,
    record: u32,
    skeletons: &[Skeleton],
    fragments: &[Fragment],
) -> Result<Vec<Kf8TocEntry>, CoreError> {
    let index = load_index(book, record)?;
    index
        .entries
        .iter()
        .map(|entry| {
            let target = one(entry, 1, "NCX target")? as usize;
            let file = target_file(target, skeletons, fragments)
                .ok_or_else(|| invalid("NCX target does not map to a KF8 file"))?;
            let label = String::from_utf8_lossy(&entry.label).trim().to_string();
            if label.is_empty() {
                return Err(invalid("NCX entry has an empty label"));
            }
            let depth = entry
                .values(4)
                .and_then(|values| values.first())
                .copied()
                .unwrap_or(1);
            Ok(Kf8TocEntry {
                label,
                file,
                depth: usize::try_from(depth.max(1)).unwrap_or(usize::MAX),
            })
        })
        .collect()
}

fn target_file(target: usize, skeletons: &[Skeleton], fragments: &[Fragment]) -> Option<usize> {
    skeletons
        .iter()
        .position(|skeleton| in_range(target, skeleton.start, skeleton.length))
        .or_else(|| {
            fragments
                .iter()
                .find(|fragment| in_range(target, fragment.start, fragment.length))
                .map(|fragment| fragment.file)
        })
}

fn in_range(target: usize, start: usize, length: usize) -> bool {
    start
        .checked_add(length)
        .is_some_and(|end| target >= start && target < end)
}

fn one(entry: &IndexEntry, tag: u8, name: &str) -> Result<u32, CoreError> {
    let values = entry
        .values(tag)
        .ok_or_else(|| invalid(&format!("missing {name}")))?;
    match values {
        [value] => Ok(*value),
        _ => Err(invalid(&format!("invalid {name}"))),
    }
}

fn pair(entry: &IndexEntry, tag: u8, name: &str) -> Result<(u32, u32), CoreError> {
    let values = entry
        .values(tag)
        .ok_or_else(|| invalid(&format!("missing {name}")))?;
    match values {
        [first, second] => Ok((*first, *second)),
        _ => Err(invalid(&format!("invalid {name}"))),
    }
}

fn slice<'a>(
    bytes: &'a [u8],
    start: usize,
    length: usize,
    name: &str,
) -> Result<&'a [u8], CoreError> {
    let end = start
        .checked_add(length)
        .ok_or_else(|| invalid(&format!("{name} byte range overflow")))?;
    bytes
        .get(start..end)
        .ok_or_else(|| invalid(&format!("{name} byte range is outside flow 0")))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, CoreError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| invalid("truncated KF8 field"))?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidPublication(message.into())
}

#[cfg(test)]
#[path = "kf8_tests.rs"]
mod tests;
