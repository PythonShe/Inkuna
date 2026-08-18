//! Combo-aware assembled MOBI book surface.

use std::path::Path;

use encoding_rs::WINDOWS_1252;

use super::header::{parse_headers, ExthRecord, Headers};
use super::huffcdic::HuffCdic;
use super::palmdoc;
use super::pdb::PalmDatabase;
use crate::CoreError;

const MAX_RECORD_TEXT_BYTES: usize = 64 * 1024 * 8;
const MAX_TOTAL_TEXT_BYTES: usize = 96 * 1024 * 1024;
const MAX_HEADER_RECORD_BYTES: usize = 64 * 1024 * 1024;
const MAX_HUFF_TABLE_BYTES: usize = 64 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_AUTHORS: usize = 1_000;
const MAX_METADATA_VALUE_BYTES: usize = 2_048;

struct View {
    header_index: usize,
    headers: Headers,
}

pub(crate) struct Kf8Pointers {
    pub(crate) fdst: u32,
    pub(crate) flow_count: u32,
    pub(crate) fragment_index: u32,
    pub(crate) skeleton_index: u32,
    pub(crate) ncx_index: Option<u32>,
    pub(crate) guide_index: Option<u32>,
}

/// Parsed MOBI container. Combo files transparently select their KF8 view.
#[allow(dead_code)]
pub(crate) struct MobiBook {
    database: PalmDatabase,
    view: View,
    mobi6_view: Option<View>,
    kf8_boundary: Option<u32>,
}

#[allow(dead_code)]
impl MobiBook {
    pub(crate) fn open(path: &Path) -> Result<Self, CoreError> {
        let database = PalmDatabase::open(path)?;
        if !database.is_mobipocket() {
            return Err(invalid("Palm database is not a BOOKMOBI publication"));
        }
        let primary = parse_view(&database, 0)?;
        refuse_drm(&primary.headers)?;
        let kf8_boundary = boundary(&primary.headers.exth)?;

        let (view, mobi6_view) = if let Some(boundary) = kf8_boundary {
            let boundary_index = boundary as usize;
            let boundary_prefix = database.record_prefix(boundary_index, 8)?;
            let header_index = if boundary_prefix == b"BOUNDARY" {
                boundary_index
                    .checked_add(1)
                    .ok_or_else(|| invalid("KF8 boundary index overflow"))?
            } else {
                boundary_index
            };
            let candidate = parse_view(&database, header_index)?;
            refuse_drm(&candidate.headers)?;
            (candidate, Some(primary))
        } else {
            (primary, None)
        };

        Ok(Self {
            database,
            view,
            mobi6_view,
            kf8_boundary,
        })
    }

    pub(crate) fn is_kf8(&self) -> bool {
        self.view.headers.mobi.file_version >= 8
    }

    pub(crate) fn kf8_boundary(&self) -> Option<u32> {
        self.kf8_boundary
    }

    pub(crate) fn select_mobi6(&mut self) {
        if let Some(view) = self.mobi6_view.take() {
            self.view = view;
        }
    }

    pub(crate) fn kf8_pointers(&self) -> Result<Kf8Pointers, CoreError> {
        if !self.is_kf8() {
            return Err(invalid("selected MOBI view is not KF8"));
        }
        let mobi = &self.view.headers.mobi;
        let (fdst, flow_count) = mobi
            .fdst
            .ok_or_else(|| invalid("KF8 MOBI header has no FDST pointer"))?;
        Ok(Kf8Pointers {
            fdst,
            flow_count,
            fragment_index: mobi
                .fragment_index
                .ok_or_else(|| invalid("KF8 MOBI header has no FRAG index pointer"))?,
            skeleton_index: mobi
                .skeleton_index
                .ok_or_else(|| invalid("KF8 MOBI header has no SKEL index pointer"))?,
            ncx_index: mobi.ncx_index,
            guide_index: mobi.guide_index,
        })
    }

    pub(crate) fn relative_record(&self, relative: u32) -> Result<&[u8], CoreError> {
        let index = self
            .view
            .header_index
            .checked_add(relative as usize)
            .ok_or_else(|| invalid("KF8 record index overflow"))?;
        self.database.record(index)
    }

    pub(crate) fn text(&self) -> Result<Vec<u8>, CoreError> {
        let palmdoc_header = &self.view.headers.palmdoc;
        let text_count = usize::from(palmdoc_header.record_count);
        let mut output =
            Vec::with_capacity((palmdoc_header.text_length as usize).min(MAX_TOTAL_TEXT_BYTES));
        let huff = if palmdoc_header.compression == 17_480 {
            Some(self.huff_decoder()?)
        } else {
            None
        };
        let mut raw_text_bytes = 0usize;

        for relative in 1..=text_count {
            let index = self
                .view
                .header_index
                .checked_add(relative)
                .ok_or_else(|| invalid("MOBI text record index overflow"))?;
            let raw_length = self.database.record_len(index)?;
            raw_text_bytes = raw_text_bytes
                .checked_add(raw_length)
                .ok_or_else(|| invalid("MOBI text record lengths overflow"))?;
            if raw_text_bytes > MAX_TOTAL_TEXT_BYTES {
                return Err(invalid(
                    "MOBI compressed text exceeds the 96 MiB input limit",
                ));
            }
            let raw = self.database.record(index)?;
            let compressed = strip_trailing(raw, self.view.headers.mobi.extra_data_flags)?;
            let record = match palmdoc_header.compression {
                1 => {
                    if compressed.len() > MAX_RECORD_TEXT_BYTES {
                        return Err(invalid("MOBI text record exceeds the per-record limit"));
                    }
                    compressed.to_vec()
                }
                2 => palmdoc::decompress(compressed, MAX_RECORD_TEXT_BYTES)?,
                17_480 => huff
                    .as_ref()
                    .ok_or_else(|| invalid("missing HUFF/CDIC decoder"))?
                    .decompress(compressed, MAX_RECORD_TEXT_BYTES)?,
                _ => return Err(invalid("unsupported MOBI text compression")),
            };
            if output
                .len()
                .checked_add(record.len())
                .is_none_or(|length| length > MAX_TOTAL_TEXT_BYTES)
            {
                return Err(invalid("MOBI text exceeds the 96 MiB decompression limit"));
            }
            output.extend_from_slice(&record);
        }
        if output.len() != palmdoc_header.text_length as usize {
            return Err(invalid(
                "decompressed MOBI text length does not match its header",
            ));
        }
        Ok(output)
    }

    pub(crate) fn encoding(&self) -> u32 {
        self.view.headers.mobi.encoding
    }

    pub(crate) fn title(&self) -> String {
        if let Some(title) = self.exth_values(503).next() {
            return self.decode_metadata(title);
        }
        if let Some((offset, length)) = self.view.headers.mobi.fullname {
            if let Ok(record0) = self.database.record(self.view.header_index) {
                let start = offset as usize;
                if let Some(end) = start.checked_add(length as usize) {
                    if let Some(name) = record0.get(start..end) {
                        return self.decode_metadata(name);
                    }
                }
            }
        }
        self.decode(self.database.name())
    }

    pub(crate) fn authors(&self) -> Vec<String> {
        self.exth_values(100)
            .take(MAX_AUTHORS)
            .map(|author| self.decode_metadata(author))
            .collect()
    }

    pub(crate) fn language(&self) -> Option<String> {
        let primary = self.view.headers.mobi.locale & 0x03ff;
        let language = match primary {
            0x04 => "zh",
            0x07 => "de",
            0x09 => "en",
            0x0a => "es",
            0x0c => "fr",
            0x10 => "it",
            0x11 => "ja",
            0x12 => "ko",
            0x16 => "pt",
            0x19 => "ru",
            _ => return None,
        };
        Some(language.to_string())
    }

    pub(crate) fn image(&self, recindex_1_based: u32) -> Option<&[u8]> {
        self.image_record_index(recindex_1_based)
            .and_then(|index| self.image_at(index))
    }

    pub(crate) fn image_len(&self, recindex_1_based: u32) -> Option<usize> {
        let index = self.image_record_index(recindex_1_based)?;
        self.database
            .record_len(index)
            .ok()
            .filter(|length| *length <= MAX_IMAGE_BYTES)
    }

    pub(crate) fn cover(&self) -> Option<&[u8]> {
        self.image(self.cover_recindex()?)
    }

    pub(crate) fn cover_len(&self) -> Option<usize> {
        self.image_len(self.cover_recindex()?)
    }

    fn huff_decoder(&self) -> Result<HuffCdic, CoreError> {
        let (raw_offset, count) = self
            .view
            .headers
            .mobi
            .huff_records
            .ok_or_else(|| invalid("HUFF/CDIC compression has no table records"))?;
        let first = self
            .view
            .header_index
            .checked_add(raw_offset as usize)
            .ok_or_else(|| invalid("MOBI resource record index overflow"))?;
        let mut table_bytes = self.database.record_len(first)?;
        if table_bytes > MAX_HUFF_TABLE_BYTES {
            return Err(invalid("HUFF/CDIC tables exceed the 64 MiB limit"));
        }
        let huff = self.database.record(first)?;
        if !huff.starts_with(b"HUFF") {
            return Err(invalid("MOBI resource record has the wrong signature"));
        }
        let mut cdics = Vec::new();
        for relative in 1..count as usize {
            let index = first
                .checked_add(relative)
                .ok_or_else(|| invalid("CDIC record index overflow"))?;
            table_bytes = table_bytes
                .checked_add(self.database.record_len(index)?)
                .ok_or_else(|| invalid("HUFF/CDIC table lengths overflow"))?;
            if table_bytes > MAX_HUFF_TABLE_BYTES {
                return Err(invalid("HUFF/CDIC tables exceed the 64 MiB limit"));
            }
            cdics.push(self.database.record(index)?);
        }
        HuffCdic::parse(huff, &cdics)
    }

    fn exth_values(&self, kind: u32) -> impl Iterator<Item = &[u8]> {
        self.view
            .headers
            .exth
            .iter()
            .filter(move |record| record.kind == kind)
            .map(|record| record.data.as_slice())
    }

    fn exth_u32(&self, kind: u32) -> Option<u32> {
        let value = self.exth_values(kind).next()?;
        let bytes: [u8; 4] = value.try_into().ok()?;
        Some(u32::from_be_bytes(bytes))
    }

    fn decode(&self, bytes: &[u8]) -> String {
        let decoded = if self.encoding() == 1252 {
            WINDOWS_1252
                .decode_without_bom_handling(bytes)
                .0
                .into_owned()
        } else {
            String::from_utf8_lossy(bytes).into_owned()
        };
        decoded.trim_end_matches('\0').to_string()
    }

    fn decode_metadata(&self, bytes: &[u8]) -> String {
        let mut source_end = bytes.len().min(MAX_METADATA_VALUE_BYTES);
        if self.encoding() != 1252 && source_end < bytes.len() {
            while source_end > 0 && bytes[source_end] & 0xc0 == 0x80 {
                source_end -= 1;
            }
        }
        let mut decoded = self.decode(&bytes[..source_end]);
        if decoded.len() > MAX_METADATA_VALUE_BYTES {
            let mut end = MAX_METADATA_VALUE_BYTES;
            while !decoded.is_char_boundary(end) {
                end -= 1;
            }
            decoded.truncate(end);
        }
        decoded
    }

    fn image_at(&self, index: usize) -> Option<&[u8]> {
        if self.database.record_len(index).ok()? > MAX_IMAGE_BYTES {
            return None;
        }
        let bytes = self.database.record(index).ok()?;
        is_image(bytes).then_some(bytes)
    }

    fn image_record_index(&self, recindex_1_based: u32) -> Option<usize> {
        let first = self.view.headers.mobi.first_image_index?;
        let relative = first.checked_add(recindex_1_based.checked_sub(1)?)? as usize;
        self.view.header_index.checked_add(relative)
    }

    fn cover_recindex(&self) -> Option<u32> {
        self.exth_u32(201)
            .or_else(|| self.exth_u32(202))?
            .checked_add(1)
    }
}

fn parse_view(database: &PalmDatabase, header_index: usize) -> Result<View, CoreError> {
    if database.record_len(header_index)? > MAX_HEADER_RECORD_BYTES {
        return Err(invalid("MOBI header record exceeds the 64 MiB limit"));
    }
    Ok(View {
        header_index,
        headers: parse_headers(database.record(header_index)?)?,
    })
}

fn refuse_drm(headers: &Headers) -> Result<(), CoreError> {
    if headers.palmdoc.encryption_type != 0 {
        return Err(invalid(
            "DRM-protected MOBI/AZW3 publications are not supported",
        ));
    }
    Ok(())
}

fn boundary(records: &[ExthRecord]) -> Result<Option<u32>, CoreError> {
    let Some(record) = records.iter().find(|record| record.kind == 121) else {
        return Ok(None);
    };
    let bytes: [u8; 4] = record
        .data
        .as_slice()
        .try_into()
        .map_err(|_| invalid("EXTH 121 KF8 boundary must be four bytes"))?;
    let boundary = u32::from_be_bytes(bytes);
    Ok((boundary != u32::MAX).then_some(boundary))
}

fn strip_trailing(record: &[u8], flags: u32) -> Result<&[u8], CoreError> {
    let mut end = record.len();
    for bit in (1..32).rev() {
        if flags & (1u32 << bit) != 0 {
            let size = backward_varint(&record[..end])?;
            if size == 0 || size > end {
                return Err(invalid("invalid MOBI sized trailing entry"));
            }
            end -= size;
        }
    }
    if flags & 1 != 0 {
        let last = *record
            .get(end.wrapping_sub(1))
            .ok_or_else(|| invalid("missing MOBI multibyte trailing entry"))?;
        let size = usize::from(last & 0x03) + 1;
        if size > end {
            return Err(invalid("invalid MOBI multibyte trailing entry"));
        }
        end -= size;
    }
    Ok(&record[..end])
}

fn backward_varint(bytes: &[u8]) -> Result<usize, CoreError> {
    let mut value = 0usize;
    for (shift, byte) in bytes.iter().rev().take(5).enumerate() {
        value |= usize::from(byte & 0x7f) << (shift * 7);
        if byte & 0x80 != 0 {
            return Ok(value);
        }
    }
    Err(invalid("unterminated MOBI backward variable-width integer"))
}

fn is_image(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xff, 0xd8, 0xff])
        || bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || bytes.starts_with(b"BM")
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidPublication(message.to_string())
}

#[cfg(test)]
#[path = "book_tests.rs"]
mod tests;
