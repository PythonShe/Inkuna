//! PalmDOC, MOBI, and EXTH header parsing.

use crate::CoreError;

const PALMDOC_LEN: usize = 16;
const MIN_MOBI_LEN: usize = 96;
const MAX_EXTH_RECORDS: usize = 4_096;
const MAX_EXTH_RECORD_BYTES: usize = 1024 * 1024;

pub(super) struct PalmDocHeader {
    pub(super) compression: u16,
    pub(super) text_length: u32,
    pub(super) record_count: u16,
    #[allow(dead_code)]
    pub(super) record_size: u16,
    pub(super) encryption_type: u16,
}

pub(super) struct MobiHeader {
    pub(super) encoding: u32,
    pub(super) file_version: u32,
    pub(super) fullname: Option<(u32, u32)>,
    pub(super) locale: u32,
    pub(super) first_image_index: Option<u32>,
    pub(super) huff_records: Option<(u32, u32)>,
    pub(super) extra_data_flags: u32,
    pub(super) fdst: Option<(u32, u32)>,
    pub(super) ncx_index: Option<u32>,
    pub(super) fragment_index: Option<u32>,
    pub(super) skeleton_index: Option<u32>,
    pub(super) guide_index: Option<u32>,
}

pub(super) struct ExthRecord {
    pub(super) kind: u32,
    pub(super) data: Vec<u8>,
}

pub(super) struct Headers {
    pub(super) palmdoc: PalmDocHeader,
    pub(super) mobi: MobiHeader,
    pub(super) exth: Vec<ExthRecord>,
}

pub(super) fn parse_headers(record: &[u8]) -> Result<Headers, CoreError> {
    let palmdoc = parse_palmdoc(record)?;
    let mobi_length = read_u32(record, 20)? as usize;
    if record.get(16..20) != Some(b"MOBI".as_slice()) {
        return Err(invalid("record 0 is missing the MOBI header"));
    }
    if mobi_length < MIN_MOBI_LEN {
        return Err(invalid("MOBI header is too short"));
    }
    let mobi_end = PALMDOC_LEN
        .checked_add(mobi_length)
        .ok_or_else(|| invalid("MOBI header length overflow"))?;
    if mobi_end > record.len() {
        return Err(invalid("truncated MOBI header"));
    }

    let fullname_offset = read_u32(record, 84)?;
    let fullname_length = read_u32(record, 88)?;
    let fullname = (fullname_length != 0 && fullname_offset != u32::MAX)
        .then_some((fullname_offset, fullname_length));
    let first_image = read_u32(record, 108)?;
    let first_image_index = (first_image != u32::MAX).then_some(first_image);
    let huff_records = if mobi_length >= 104 {
        let offset = read_u32(record, 112)?;
        let count = read_u32(record, 116)?;
        (offset != u32::MAX && count != 0 && count != u32::MAX).then_some((offset, count))
    } else {
        None
    };
    let exth_flags = if mobi_length >= 116 {
        read_u32(record, 128)?
    } else {
        0
    };
    let file_version = read_u32(record, 36)?;
    let extra_data_flags = if mobi_length >= 228 && file_version >= 5 {
        u32::from(read_u16(record, 242)?)
    } else {
        0
    };
    let exth = if exth_flags & 0x40 != 0 {
        parse_exth(record, mobi_end)?
    } else {
        Vec::new()
    };
    let fdst = if file_version >= 8 && mobi_length >= 184 {
        let index = read_u32(record, 192)?;
        let count = read_u32(record, 196)?;
        (index != u32::MAX && count != 0 && count != u32::MAX).then_some((index, count))
    } else {
        None
    };

    Ok(Headers {
        palmdoc,
        mobi: MobiHeader {
            encoding: read_u32(record, 28)?,
            file_version,
            fullname,
            locale: read_u32(record, 92)?,
            first_image_index,
            huff_records,
            extra_data_flags,
            fdst,
            ncx_index: optional_index(record, mobi_length, 232, 244)?,
            fragment_index: optional_index(record, mobi_length, 236, 248)?,
            skeleton_index: optional_index(record, mobi_length, 240, 252)?,
            guide_index: optional_index(record, mobi_length, 248, 260)?,
        },
        exth,
    })
}

fn optional_index(
    record: &[u8],
    mobi_length: usize,
    minimum_length: usize,
    offset: usize,
) -> Result<Option<u32>, CoreError> {
    if mobi_length < minimum_length {
        return Ok(None);
    }
    let value = read_u32(record, offset)?;
    Ok((value != u32::MAX).then_some(value))
}

fn parse_palmdoc(record: &[u8]) -> Result<PalmDocHeader, CoreError> {
    if record.len() < PALMDOC_LEN {
        return Err(invalid("truncated PalmDOC header"));
    }
    let compression = read_u16(record, 0)?;
    if !matches!(compression, 1 | 2 | 17_480) {
        return Err(invalid("unsupported PalmDOC compression"));
    }
    let encryption_type = read_u16(record, 12)?;
    Ok(PalmDocHeader {
        compression,
        text_length: read_u32(record, 4)?,
        record_count: read_u16(record, 8)?,
        record_size: read_u16(record, 10)?,
        encryption_type,
    })
}

fn parse_exth(record: &[u8], offset: usize) -> Result<Vec<ExthRecord>, CoreError> {
    if record.get(offset..offset + 4) != Some(b"EXTH".as_slice()) {
        return Err(invalid("MOBI header flags announce a missing EXTH header"));
    }
    let length = read_u32(record, offset + 4)? as usize;
    if length < 12 {
        return Err(invalid("EXTH header length is too short"));
    }
    let end = offset
        .checked_add(length)
        .ok_or_else(|| invalid("EXTH header length overflow"))?;
    if end > record.len() {
        return Err(invalid("truncated EXTH header"));
    }
    let count = read_u32(record, offset + 8)? as usize;
    if count > MAX_EXTH_RECORDS {
        return Err(invalid("EXTH record count exceeds 4096"));
    }

    let mut records = Vec::with_capacity(count);
    let mut cursor = offset + 12;
    for _ in 0..count {
        let kind = read_u32(record, cursor)?;
        let length = read_u32(record, cursor + 4)? as usize;
        if !(8..=MAX_EXTH_RECORD_BYTES).contains(&length) {
            return Err(invalid("EXTH record length is invalid or exceeds 1 MiB"));
        }
        let next = cursor
            .checked_add(length)
            .ok_or_else(|| invalid("EXTH record length overflow"))?;
        if next > end {
            return Err(invalid("EXTH record extends past the EXTH header"));
        }
        records.push(ExthRecord {
            kind,
            data: record[cursor + 8..next].to_vec(),
        });
        cursor = next;
    }
    Ok(records)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, CoreError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| invalid("truncated MOBI header field"))?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, CoreError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| invalid("truncated MOBI header field"))?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidPublication(message.to_string())
}

#[cfg(test)]
#[path = "header_tests.rs"]
mod tests;
