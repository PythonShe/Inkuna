//! PalmDOC's block-local LZ77/byte-pair decompressor.

use crate::FormatError;

pub(super) fn decompress(input: &[u8], output_cap: usize) -> Result<Vec<u8>, FormatError> {
    let mut output = Vec::with_capacity(input.len().min(output_cap));
    let mut cursor = 0;

    while cursor < input.len() {
        let marker = input[cursor];
        cursor += 1;
        match marker {
            0x00 | 0x09..=0x7f => push(&mut output, marker, output_cap)?,
            0x01..=0x08 => {
                let count = usize::from(marker);
                let end = cursor.checked_add(count).ok_or_else(truncated)?;
                let literals = input.get(cursor..end).ok_or_else(truncated)?;
                extend(&mut output, literals, output_cap)?;
                cursor = end;
            }
            0x80..=0xbf => {
                let low = *input.get(cursor).ok_or_else(truncated)?;
                cursor += 1;
                let pair = (u16::from(marker) << 8) | u16::from(low);
                let distance = usize::from((pair >> 3) & 0x07ff);
                let length = usize::from((pair & 0x0007) + 3);
                if distance == 0 || distance > output.len() {
                    return Err(FormatError::InvalidPublication(
                        "invalid PalmDOC backreference".to_string(),
                    ));
                }
                ensure_capacity(output.len(), length, output_cap)?;
                for _ in 0..length {
                    let byte = output[output.len() - distance];
                    output.push(byte);
                }
            }
            0xc0..=0xff => {
                ensure_capacity(output.len(), 2, output_cap)?;
                output.push(b' ');
                output.push(marker ^ 0x80);
            }
        }
    }

    Ok(output)
}

fn push(output: &mut Vec<u8>, byte: u8, cap: usize) -> Result<(), FormatError> {
    ensure_capacity(output.len(), 1, cap)?;
    output.push(byte);
    Ok(())
}

fn extend(output: &mut Vec<u8>, bytes: &[u8], cap: usize) -> Result<(), FormatError> {
    ensure_capacity(output.len(), bytes.len(), cap)?;
    output.extend_from_slice(bytes);
    Ok(())
}

fn ensure_capacity(current: usize, additional: usize, cap: usize) -> Result<(), FormatError> {
    if current
        .checked_add(additional)
        .is_none_or(|length| length > cap)
    {
        return Err(FormatError::InvalidPublication(format!(
            "PalmDOC record exceeds the {cap}-byte decompression limit"
        )));
    }
    Ok(())
}

fn truncated() -> FormatError {
    FormatError::InvalidPublication("truncated PalmDOC compressed record".to_string())
}

#[cfg(test)]
#[path = "palmdoc_tests.rs"]
mod tests;
