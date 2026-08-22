//! Palm Database container and lazily loaded record table.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use crate::FormatError;

const HEADER_LEN: usize = 78;
const RECORD_ENTRY_LEN: usize = 8;
const MAX_RECORDS: usize = 16_384;
const MAX_CONTAINER_BYTES: u64 = 1024 * 1024 * 1024;

enum Storage {
    #[cfg(test)]
    Memory(Vec<u8>),
    File {
        file: Mutex<File>,
        /// Lazily cached reads, retained for the life of the database.
        /// Only headers and record 0 are meant to land here — one-shot
        /// payloads (text records, image bytes) go through
        /// [`PalmDatabase::take_record`] and are never cached. The
        /// effective memory ceiling is the callers' byte caps: 96 MiB
        /// total text and 16 MiB per image (128 MiB total) enforced in
        /// `book.rs` and the `mobi`/`azw3` converters.
        records: Vec<OnceLock<Result<Box<[u8]>, StoredIoError>>>,
    },
}

struct StoredIoError {
    kind: std::io::ErrorKind,
    message: String,
}

impl From<std::io::Error> for StoredIoError {
    fn from(error: std::io::Error) -> Self {
        Self {
            kind: error.kind(),
            message: error.to_string(),
        }
    }
}

pub(super) struct PalmDatabase {
    storage: Storage,
    name: Vec<u8>,
    records: Vec<(usize, usize)>,
    is_mobipocket: bool,
}

impl PalmDatabase {
    pub(super) fn open(path: &Path) -> Result<Self, FormatError> {
        let mut file = File::open(path)?;
        let file_length = file.metadata()?.len();
        check_container_size(file_length)?;
        let file_length = usize::try_from(file_length)
            .map_err(|_| invalid("MOBI container length does not fit this platform"))?;

        let mut header = [0; HEADER_LEN];
        file.read_exact(&mut header)?;
        let record_count = record_count(&header)?;
        let table_length = table_length(record_count)?;
        let mut table = vec![0; table_length];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut table)?;
        let (name, records, is_mobipocket) = parse_layout(&table, file_length)?;
        let loaded = (0..record_count).map(|_| OnceLock::new()).collect();

        Ok(Self {
            storage: Storage::File {
                file: Mutex::new(file),
                records: loaded,
            },
            name,
            records,
            is_mobipocket,
        })
    }

    #[cfg(test)]
    pub(super) fn parse(bytes: Vec<u8>) -> Result<Self, FormatError> {
        let header = bytes
            .get(..HEADER_LEN)
            .ok_or_else(|| invalid("truncated PDB header"))?;
        let count = record_count(header)?;
        let table_length = table_length(count)?;
        let table = bytes
            .get(..table_length)
            .ok_or_else(|| invalid("truncated PDB record table"))?;
        let (name, records, is_mobipocket) = parse_layout(table, bytes.len())?;
        Ok(Self {
            storage: Storage::Memory(bytes),
            name,
            records,
            is_mobipocket,
        })
    }

    pub(super) fn name(&self) -> &[u8] {
        &self.name
    }

    pub(super) fn is_mobipocket(&self) -> bool {
        self.is_mobipocket
    }

    #[cfg(test)]
    pub(super) fn record_count(&self) -> usize {
        self.records.len()
    }

    pub(super) fn record_len(&self, index: usize) -> Result<usize, FormatError> {
        let (start, end) = self.record_range(index)?;
        Ok(end - start)
    }

    pub(super) fn record(&self, index: usize) -> Result<&[u8], FormatError> {
        let (start, end) = self.record_range(index)?;
        match &self.storage {
            #[cfg(test)]
            Storage::Memory(bytes) => Ok(&bytes[start..end]),
            Storage::File { file, records } => {
                let cell = records
                    .get(index)
                    .ok_or_else(|| invalid("PDB record index is out of range"))?;
                let loaded = cell.get_or_init(|| {
                    let mut bytes = vec![0; end - start];
                    let result = (|| -> Result<Box<[u8]>, std::io::Error> {
                        let mut file = file.lock().unwrap();
                        file.seek(SeekFrom::Start(start as u64))?;
                        file.read_exact(&mut bytes)?;
                        Ok(bytes.into_boxed_slice())
                    })();
                    result.map_err(StoredIoError::from)
                });
                match loaded {
                    Ok(bytes) => Ok(bytes),
                    Err(error) => Err(FormatError::Io(std::io::Error::new(
                        error.kind,
                        error.message.clone(),
                    ))),
                }
            }
        }
    }

    /// Reads a record into an owned buffer without populating the cache.
    /// For one-shot payloads the caller consumes immediately (text
    /// records, image bytes); headers and record 0 stay on [`record`]
    /// (Self::record) so repeated lookups hit the cache. An already
    /// cached record is cloned rather than reread from disk.
    pub(super) fn take_record(&self, index: usize) -> Result<Box<[u8]>, FormatError> {
        let (start, end) = self.record_range(index)?;
        match &self.storage {
            #[cfg(test)]
            Storage::Memory(bytes) => Ok(Box::from(&bytes[start..end])),
            Storage::File { file, records } => {
                if let Some(Ok(cached)) = records.get(index).and_then(|cell| cell.get()) {
                    return Ok(cached.clone());
                }
                let mut bytes = vec![0; end - start];
                let mut file = file.lock().unwrap();
                file.seek(SeekFrom::Start(start as u64))?;
                file.read_exact(&mut bytes)?;
                Ok(bytes.into_boxed_slice())
            }
        }
    }

    pub(super) fn record_prefix(&self, index: usize, length: usize) -> Result<Vec<u8>, FormatError> {
        let (start, end) = self.record_range(index)?;
        let length = length.min(end - start);
        match &self.storage {
            #[cfg(test)]
            Storage::Memory(bytes) => Ok(bytes[start..start + length].to_vec()),
            Storage::File { file, .. } => {
                let mut prefix = vec![0; length];
                let mut file = file.lock().unwrap();
                file.seek(SeekFrom::Start(start as u64))?;
                file.read_exact(&mut prefix)?;
                Ok(prefix)
            }
        }
    }

    fn record_range(&self, index: usize) -> Result<(usize, usize), FormatError> {
        self.records
            .get(index)
            .copied()
            .ok_or_else(|| invalid("PDB record index is out of range"))
    }
}

fn record_count(header: &[u8]) -> Result<usize, FormatError> {
    let count_bytes = header
        .get(76..78)
        .ok_or_else(|| invalid("truncated PDB header"))?;
    let count = usize::from(u16::from_be_bytes([count_bytes[0], count_bytes[1]]));
    if count > MAX_RECORDS {
        return Err(invalid("PDB record count exceeds 16384"));
    }
    Ok(count)
}

fn table_length(record_count: usize) -> Result<usize, FormatError> {
    record_count
        .checked_mul(RECORD_ENTRY_LEN)
        .and_then(|length| HEADER_LEN.checked_add(length))
        .ok_or_else(|| invalid("PDB record table length overflow"))
}

fn parse_layout(
    table: &[u8],
    file_length: usize,
) -> Result<(Vec<u8>, Vec<(usize, usize)>, bool), FormatError> {
    let header = table
        .get(..HEADER_LEN)
        .ok_or_else(|| invalid("truncated PDB header"))?;
    let count = record_count(header)?;
    let table_length = table_length(count)?;
    if table.len() < table_length {
        return Err(invalid("truncated PDB record table"));
    }

    let mut offsets = Vec::with_capacity(count);
    for index in 0..count {
        let start = HEADER_LEN + index * RECORD_ENTRY_LEN;
        let offset = read_u32(table, start)? as usize;
        if offset < table_length || offset >= file_length {
            return Err(invalid("PDB record offset is outside the data area"));
        }
        if offsets.last().is_some_and(|previous| *previous >= offset) {
            return Err(invalid("PDB record offsets are not strictly increasing"));
        }
        offsets.push(offset);
    }

    let records = offsets
        .iter()
        .enumerate()
        .map(|(index, start)| {
            (
                *start,
                offsets.get(index + 1).copied().unwrap_or(file_length),
            )
        })
        .collect();
    let name_end = header[..32]
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(32);
    Ok((
        header[..name_end].to_vec(),
        records,
        header.get(60..68) == Some(b"BOOKMOBI".as_slice()),
    ))
}

fn check_container_size(length: u64) -> Result<(), FormatError> {
    if length > MAX_CONTAINER_BYTES {
        return Err(invalid("MOBI container exceeds the 1 GiB limit"));
    }
    Ok(())
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, FormatError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| invalid("truncated PDB record entry"))?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn invalid(message: &str) -> FormatError {
    FormatError::InvalidPublication(message.to_string())
}

#[cfg(test)]
#[path = "pdb_tests.rs"]
mod tests;
