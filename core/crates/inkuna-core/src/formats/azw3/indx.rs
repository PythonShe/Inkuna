//! Bounded parsing of MOBI `INDX`, `TAGX`, and `IDXT` records.

use crate::CoreError;

const MIN_INDX_HEADER_BYTES: usize = 28;
const MIN_META_HEADER_BYTES: usize = 56;
const MAX_ENTRIES: usize = 65_536;
const MAX_TAGS_PER_ENTRY: usize = 64;
const MAX_CONTROL_BYTES: usize = 64;

pub(super) struct Index {
    pub(super) entries: Vec<IndexEntry>,
}

pub(super) struct IndexEntry {
    pub(super) label: Vec<u8>,
    tags: Vec<(u8, Vec<u32>)>,
}

impl IndexEntry {
    pub(super) fn values(&self, tag: u8) -> Option<&[u32]> {
        self.tags
            .iter()
            .find(|(candidate, _)| *candidate == tag)
            .map(|(_, values)| values.as_slice())
    }
}

struct TagDefinition {
    tag: u8,
    values_per_entry: u8,
    mask: u8,
    control_index: usize,
}

struct Tagx {
    control_bytes: usize,
    definitions: Vec<TagDefinition>,
}

#[cfg(test)]
pub(super) fn parse_records(records: &[Vec<u8>]) -> Result<Index, CoreError> {
    let borrowed = records.iter().map(Vec::as_slice).collect::<Vec<_>>();
    parse(&borrowed)
}

pub(super) fn parse(records: &[&[u8]]) -> Result<Index, CoreError> {
    let meta = records
        .first()
        .copied()
        .ok_or_else(|| invalid("missing INDX meta record"))?;
    check_magic(meta, b"INDX", "INDX meta record")?;
    let header_length = read_u32(meta, 4)? as usize;
    if header_length < MIN_META_HEADER_BYTES || header_length > meta.len() {
        return Err(invalid("invalid INDX meta header length"));
    }
    let record_count = read_u32(meta, 24)? as usize;
    let declared_entries = read_u32(meta, 36)? as usize;
    if declared_entries > MAX_ENTRIES {
        return Err(invalid("INDX entry count exceeds 65536"));
    }
    if record_count == 0 || record_count > MAX_ENTRIES {
        return Err(invalid("INDX data record count is invalid"));
    }
    let required_records = record_count
        .checked_add(1)
        .ok_or_else(|| invalid("INDX record count overflow"))?;
    if records.len() < required_records {
        return Err(invalid("truncated INDX record sequence"));
    }
    let tagx = parse_tagx(&meta[header_length..])?;

    let mut entries = Vec::with_capacity(declared_entries);
    for record in &records[1..required_records] {
        parse_data_record(record, &tagx, &mut entries)?;
    }
    if entries.len() != declared_entries {
        return Err(invalid("INDX entries do not match the declared total"));
    }
    Ok(Index { entries })
}

fn parse_tagx(bytes: &[u8]) -> Result<Tagx, CoreError> {
    check_magic(bytes, b"TAGX", "TAGX section")?;
    let length = read_u32(bytes, 4)? as usize;
    if length < 12 || length > bytes.len() || !(length - 12).is_multiple_of(4) {
        return Err(invalid("invalid TAGX section length"));
    }
    let control_bytes = read_u32(bytes, 8)? as usize;
    if !(1..=MAX_CONTROL_BYTES).contains(&control_bytes) {
        return Err(invalid("TAGX control byte count is invalid"));
    }

    let mut definitions = Vec::new();
    let mut control_index = 0usize;
    for raw in bytes[12..length].chunks_exact(4) {
        let [tag, values_per_entry, mask, end] = [raw[0], raw[1], raw[2], raw[3]];
        if end == 1 {
            if tag != 0 || values_per_entry != 0 || mask != 0 {
                return Err(invalid("malformed TAGX control-byte terminator"));
            }
            control_index = control_index
                .checked_add(1)
                .ok_or_else(|| invalid("TAGX control byte index overflow"))?;
            if control_index > control_bytes {
                return Err(invalid("TAGX defines more control bytes than declared"));
            }
            continue;
        }
        if end != 0 || values_per_entry == 0 || mask == 0 {
            return Err(invalid("malformed TAGX tag definition"));
        }
        if control_index >= control_bytes {
            return Err(invalid("TAGX tag uses an undeclared control byte"));
        }
        definitions.push(TagDefinition {
            tag,
            values_per_entry,
            mask,
            control_index,
        });
    }
    Ok(Tagx {
        control_bytes,
        definitions,
    })
}

fn parse_data_record(
    record: &[u8],
    tagx: &Tagx,
    output: &mut Vec<IndexEntry>,
) -> Result<(), CoreError> {
    check_magic(record, b"INDX", "INDX data record")?;
    let header_length = read_u32(record, 4)? as usize;
    let idxt_start = read_u32(record, 20)? as usize;
    let entry_count = read_u32(record, 24)? as usize;
    if header_length < MIN_INDX_HEADER_BYTES || header_length > idxt_start {
        return Err(invalid("invalid INDX data header length"));
    }
    if output
        .len()
        .checked_add(entry_count)
        .is_none_or(|count| count > MAX_ENTRIES)
    {
        return Err(invalid("INDX entry count exceeds 65536"));
    }
    let table_bytes = entry_count
        .checked_mul(2)
        .and_then(|length| length.checked_add(4))
        .ok_or_else(|| invalid("IDXT table length overflow"))?;
    let idxt_end = idxt_start
        .checked_add(table_bytes)
        .ok_or_else(|| invalid("IDXT table offset overflow"))?;
    if idxt_end > record.len() || record.get(idxt_start..idxt_start + 4) != Some(b"IDXT") {
        return Err(invalid("missing or truncated IDXT table"));
    }

    let mut offsets = Vec::with_capacity(entry_count);
    for index in 0..entry_count {
        let offset = read_u16(record, idxt_start + 4 + index * 2)? as usize;
        if offset < header_length || offset >= idxt_start {
            return Err(invalid("IDXT entry offset is outside the entry area"));
        }
        if offsets.last().is_some_and(|previous| *previous >= offset) {
            return Err(invalid("IDXT entry offsets are not strictly increasing"));
        }
        offsets.push(offset);
    }

    for (index, start) in offsets.iter().copied().enumerate() {
        let end = offsets.get(index + 1).copied().unwrap_or(idxt_start);
        output.push(parse_entry(&record[start..end], tagx)?);
    }
    Ok(())
}

fn parse_entry(bytes: &[u8], tagx: &Tagx) -> Result<IndexEntry, CoreError> {
    let label_length = usize::from(
        *bytes
            .first()
            .ok_or_else(|| invalid("truncated INDX entry label"))?,
    );
    let label_end = 1usize
        .checked_add(label_length)
        .ok_or_else(|| invalid("INDX entry label length overflow"))?;
    let controls_end = label_end
        .checked_add(tagx.control_bytes)
        .ok_or_else(|| invalid("INDX entry control length overflow"))?;
    let controls = bytes
        .get(label_end..controls_end)
        .ok_or_else(|| invalid("truncated INDX entry control bytes"))?;
    let label = bytes
        .get(1..label_end)
        .ok_or_else(|| invalid("truncated INDX entry label"))?
        .to_vec();
    let mut cursor = controls_end;
    let mut tag_count = 0usize;
    let mut tags = Vec::new();

    for definition in &tagx.definitions {
        let encoded_count = controls[definition.control_index] & definition.mask;
        if encoded_count == 0 {
            continue;
        }
        // A single-bit mask can only encode "present once"; a multi-bit mask
        // stores the occurrence count shifted into the mask, with the
        // all-bits-set value announcing a varint byte-length budget followed
        // by that many bytes of varint values.
        let values = if encoded_count == definition.mask && definition.mask.count_ones() > 1 {
            let budget = read_varuint(bytes, &mut cursor)? as usize;
            let end = cursor
                .checked_add(budget)
                .ok_or_else(|| invalid("INDX tag byte-length budget overflow"))?;
            let mut values = Vec::new();
            while cursor < end {
                tag_count = tag_count
                    .checked_add(1)
                    .ok_or_else(|| invalid("INDX entry tag count overflow"))?;
                if tag_count > MAX_TAGS_PER_ENTRY {
                    return Err(invalid("INDX entry exceeds 64 tags"));
                }
                values.push(read_varuint(bytes, &mut cursor)?);
            }
            if cursor != end {
                return Err(invalid("INDX tag values overrun their byte-length budget"));
            }
            values
        } else {
            let count = if encoded_count == definition.mask {
                1usize
            } else {
                usize::from(encoded_count >> definition.mask.trailing_zeros())
            };
            tag_count = tag_count
                .checked_add(count)
                .ok_or_else(|| invalid("INDX entry tag count overflow"))?;
            if tag_count > MAX_TAGS_PER_ENTRY {
                return Err(invalid("INDX entry exceeds 64 tags"));
            }
            let value_count = count
                .checked_mul(usize::from(definition.values_per_entry))
                .ok_or_else(|| invalid("INDX tag value count overflow"))?;
            let mut values = Vec::with_capacity(value_count);
            for _ in 0..value_count {
                values.push(read_varuint(bytes, &mut cursor)?);
            }
            values
        };
        tags.push((definition.tag, values));
    }
    if cursor != bytes.len() {
        return Err(invalid("INDX entry has undecoded trailing bytes"));
    }
    Ok(IndexEntry { label, tags })
}

pub(super) fn read_varuint(bytes: &[u8], cursor: &mut usize) -> Result<u32, CoreError> {
    let mut value = 0u32;
    for _ in 0..5 {
        let byte = *bytes
            .get(*cursor)
            .ok_or_else(|| invalid("truncated INDX variable-width integer"))?;
        *cursor += 1;
        value = value
            .checked_shl(7)
            .and_then(|value| value.checked_add(u32::from(byte & 0x7f)))
            .ok_or_else(|| invalid("INDX variable-width integer overflow"))?;
        if byte & 0x80 != 0 {
            return Ok(value);
        }
    }
    Err(invalid("unterminated INDX variable-width integer"))
}

fn check_magic(bytes: &[u8], magic: &[u8; 4], name: &str) -> Result<(), CoreError> {
    if bytes.get(..4) != Some(magic.as_slice()) {
        return Err(invalid(&format!("{name} has the wrong signature")));
    }
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, CoreError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| invalid("truncated INDX field"))?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, CoreError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| invalid("truncated INDX field"))?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidPublication(message.into())
}

#[cfg(test)]
#[path = "indx_tests.rs"]
mod tests;
