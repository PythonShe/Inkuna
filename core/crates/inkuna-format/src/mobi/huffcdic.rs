//! Bounded HUFF/CDIC table parsing and dictionary expansion.

use crate::FormatError;

const MAX_DICTIONARY_ENTRIES: usize = 1024 * 1024;
const MAX_EXPANSION_DEPTH: usize = 64;

#[derive(Clone, Copy)]
struct CacheEntry {
    code_length: usize,
    terminal: bool,
    max_code: u64,
}

struct Phrase {
    terminal: bool,
    data: Vec<u8>,
}

pub(super) struct HuffCdic {
    cache: [CacheEntry; 256],
    min_codes: [u64; 33],
    max_codes: [u64; 33],
    phrases: Vec<Phrase>,
}

impl HuffCdic {
    pub(super) fn parse(huff: &[u8], cdic_records: &[&[u8]]) -> Result<Self, FormatError> {
        if huff.get(..4) != Some(b"HUFF".as_slice()) || read_u32(huff, 4)? != 24 {
            return Err(invalid("invalid HUFF record header"));
        }
        let cache_offset = read_u32(huff, 8)? as usize;
        let base_offset = read_u32(huff, 12)? as usize;
        require_range(huff, cache_offset, 256 * 4, "truncated HUFF cache table")?;
        require_range(huff, base_offset, 32 * 8, "truncated HUFF base table")?;

        let mut cache = [CacheEntry {
            code_length: 0,
            terminal: false,
            max_code: 0,
        }; 256];
        for (index, entry) in cache.iter_mut().enumerate() {
            let raw = read_u32(huff, cache_offset + index * 4)?;
            let code_length = (raw & 0x1f) as usize;
            if !(1..=32).contains(&code_length) {
                return Err(invalid("HUFF cache table has an invalid code length"));
            }
            let terminal = raw & 0x80 != 0;
            // Codes of 8 bits or fewer resolve entirely inside the cache and
            // must be terminal; longer codes may still carry the terminal
            // flag (MOBI writers emit both), so only its absence is invalid.
            if code_length <= 8 && !terminal {
                return Err(invalid(
                    "HUFF cache entry for a short code is missing its terminal flag",
                ));
            }
            let mut max_code = u64::from(raw >> 8);
            if terminal {
                max_code = expanded_max_code(max_code, code_length)?;
            }
            *entry = CacheEntry {
                code_length,
                terminal,
                max_code,
            };
        }

        let mut min_codes = [0; 33];
        let mut max_codes = [0; 33];
        for code_length in 1..=32 {
            let raw_min = read_u32(huff, base_offset + (code_length - 1) * 8)?;
            let raw_max = read_u32(huff, base_offset + (code_length - 1) * 8 + 4)?;
            let shift = 32 - code_length;
            min_codes[code_length] = u64::from(raw_min) << shift;
            max_codes[code_length] = expanded_max_code(u64::from(raw_max), code_length)?;
        }

        let phrases = parse_cdics(cdic_records)?;
        Ok(Self {
            cache,
            min_codes,
            max_codes,
            phrases,
        })
    }

    pub(super) fn decompress<'a>(
        &'a self,
        compressed: &'a [u8],
        output_cap: usize,
    ) -> Result<Vec<u8>, FormatError> {
        enum Work<'a> {
            Compressed(&'a [u8], usize),
            Literal(&'a [u8]),
        }

        let mut output = Vec::with_capacity(compressed.len().min(output_cap));
        let mut stack = vec![Work::Compressed(compressed, 0)];
        let mut expansion_steps = 0usize;
        let work_cap = output_cap.saturating_mul(4).clamp(1024, 1_000_000);

        while let Some(work) = stack.pop() {
            match work {
                Work::Literal(bytes) => {
                    ensure_output(output.len(), bytes.len(), output_cap)?;
                    output.extend_from_slice(bytes);
                }
                Work::Compressed(bytes, depth) => {
                    if depth > MAX_EXPANSION_DEPTH {
                        return Err(invalid("HUFF/CDIC expansion depth exceeded"));
                    }
                    let indices = self.decode_indices(bytes, work_cap)?;
                    for index in indices.into_iter().rev() {
                        expansion_steps = expansion_steps.saturating_add(1);
                        if expansion_steps > work_cap {
                            return Err(invalid("HUFF/CDIC expansion budget exceeded"));
                        }
                        let phrase = self
                            .phrases
                            .get(index)
                            .ok_or_else(|| invalid("HUFF code references a missing CDIC phrase"))?;
                        if phrase.terminal {
                            stack.push(Work::Literal(&phrase.data));
                        } else {
                            stack.push(Work::Compressed(&phrase.data, depth + 1));
                        }
                    }
                }
            }
        }
        Ok(output)
    }

    fn decode_indices(&self, bytes: &[u8], symbol_cap: usize) -> Result<Vec<usize>, FormatError> {
        let total_bits = bytes
            .len()
            .checked_mul(8)
            .ok_or_else(|| invalid("HUFF bit length overflow"))?;
        let mut bit_offset = 0usize;
        let mut indices = Vec::new();
        while bit_offset < total_bits {
            let code = u64::from(peek_u32(bytes, bit_offset));
            let entry = self.cache[(code >> 24) as usize];
            let mut code_length = entry.code_length;
            let max_code = if entry.terminal {
                entry.max_code
            } else {
                while code_length <= 32 && code < self.min_codes[code_length] {
                    code_length += 1;
                }
                if code_length > 32 {
                    return Err(invalid("HUFF bitstream has no matching code"));
                }
                self.max_codes[code_length]
            };
            if code_length > total_bits - bit_offset {
                break;
            }
            if max_code < code {
                return Err(invalid("truncated or invalid HUFF bitstream"));
            }
            let shift = 32 - code_length;
            let index = usize::try_from((max_code - code) >> shift)
                .map_err(|_| invalid("HUFF dictionary index overflows"))?;
            indices.push(index);
            if indices.len() > symbol_cap {
                return Err(invalid("HUFF/CDIC expansion budget exceeded"));
            }
            bit_offset += code_length;
        }
        Ok(indices)
    }
}

fn parse_cdics(records: &[&[u8]]) -> Result<Vec<Phrase>, FormatError> {
    let mut phrases = Vec::new();
    let mut expected_total = None;
    for record in records {
        if record.get(..4) != Some(b"CDIC".as_slice()) || read_u32(record, 4)? != 16 {
            return Err(invalid("invalid CDIC record header"));
        }
        let total = read_u32(record, 8)? as usize;
        if total > MAX_DICTIONARY_ENTRIES {
            return Err(invalid("CDIC dictionary exceeds the entry limit"));
        }
        if expected_total
            .replace(total)
            .is_some_and(|expected| expected != total)
        {
            return Err(invalid("CDIC records disagree on dictionary size"));
        }
        let bits = read_u32(record, 12)? as usize;
        if bits > 16 {
            return Err(invalid("CDIC code length exceeds 16 bits"));
        }
        let entries = (1usize << bits).min(total.saturating_sub(phrases.len()));
        require_range(record, 16, entries * 2, "truncated CDIC index table")?;
        for index in 0..entries {
            let relative = read_u16(record, 16 + index * 2)? as usize;
            let position = 16usize
                .checked_add(relative)
                .ok_or_else(|| invalid("CDIC phrase offset overflow"))?;
            let encoded_length = read_u16(record, position)?;
            let length = usize::from(encoded_length & 0x7fff);
            let start = position + 2;
            let end = start
                .checked_add(length)
                .ok_or_else(|| invalid("CDIC phrase length overflow"))?;
            let data = record
                .get(start..end)
                .ok_or_else(|| invalid("truncated CDIC phrase"))?
                .to_vec();
            phrases.push(Phrase {
                terminal: encoded_length & 0x8000 != 0,
                data,
            });
        }
    }
    if phrases.len() != expected_total.unwrap_or(0) {
        return Err(invalid("CDIC dictionary is incomplete"));
    }
    Ok(phrases)
}

fn expanded_max_code(raw: u64, code_length: usize) -> Result<u64, FormatError> {
    (raw + 1)
        .checked_shl((32 - code_length) as u32)
        .and_then(|value| value.checked_sub(1))
        .ok_or_else(|| invalid("HUFF maximum code overflows"))
}

fn peek_u32(bytes: &[u8], bit_offset: usize) -> u32 {
    let byte_offset = bit_offset / 8;
    let skipped = bit_offset % 8;
    let mut window = 0u64;
    for index in 0..5 {
        window = (window << 8) | u64::from(bytes.get(byte_offset + index).copied().unwrap_or(0));
    }
    ((window >> (8 - skipped)) & u64::from(u32::MAX)) as u32
}

fn ensure_output(current: usize, additional: usize, cap: usize) -> Result<(), FormatError> {
    if current
        .checked_add(additional)
        .is_none_or(|length| length > cap)
    {
        return Err(invalid("HUFF/CDIC decompression limit exceeded"));
    }
    Ok(())
}

fn require_range(
    bytes: &[u8],
    start: usize,
    length: usize,
    message: &str,
) -> Result<(), FormatError> {
    if start
        .checked_add(length)
        .is_none_or(|end| end > bytes.len())
    {
        return Err(invalid(message));
    }
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, FormatError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| invalid("truncated HUFF/CDIC field"))?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, FormatError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| invalid("truncated HUFF/CDIC field"))?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn invalid(message: &str) -> FormatError {
    FormatError::InvalidPublication(message.to_string())
}

#[cfg(test)]
#[path = "huffcdic_tests.rs"]
mod tests;
