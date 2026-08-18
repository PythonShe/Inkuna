use std::path::Path;

pub(crate) struct MobiTestBuilder {
    version: u32,
    compression: u16,
    encryption: u16,
    encoding: u32,
    locale: u32,
    name: Vec<u8>,
    fullname: Option<Vec<u8>>,
    text_records: Vec<Vec<u8>>,
    text_length: Option<u32>,
    huff_records: Vec<Vec<u8>>,
    exth: Vec<(u32, Vec<u8>)>,
    images: Vec<Vec<u8>>,
    extra_data_flags: u32,
    trailers: Vec<Vec<u8>>,
    kf8: Option<Box<MobiTestBuilder>>,
}

impl MobiTestBuilder {
    pub(crate) fn new(version: u32) -> Self {
        Self {
            version,
            compression: 1,
            encryption: 0,
            encoding: 65001,
            locale: 9,
            name: b"Test Book".to_vec(),
            fullname: None,
            text_records: vec![b"test".to_vec()],
            text_length: None,
            huff_records: Vec::new(),
            exth: Vec::new(),
            images: Vec::new(),
            extra_data_flags: 0,
            trailers: Vec::new(),
            kf8: None,
        }
    }

    pub(crate) fn name(&mut self, name: &[u8]) -> &mut Self {
        self.name = name.to_vec();
        self
    }

    pub(crate) fn compression(&mut self, compression: u16) -> &mut Self {
        self.compression = compression;
        self
    }

    pub(crate) fn encryption(&mut self, encryption: u16) -> &mut Self {
        self.encryption = encryption;
        self
    }

    pub(crate) fn encoding(&mut self, encoding: u32) -> &mut Self {
        self.encoding = encoding;
        self
    }

    pub(crate) fn locale(&mut self, locale: u32) -> &mut Self {
        self.locale = locale;
        self
    }

    pub(crate) fn fullname(&mut self, fullname: &[u8]) -> &mut Self {
        self.fullname = Some(fullname.to_vec());
        self
    }

    pub(crate) fn text_records(&mut self, records: Vec<Vec<u8>>) -> &mut Self {
        self.text_records = records;
        self
    }

    pub(crate) fn text_length(&mut self, length: u32) -> &mut Self {
        self.text_length = Some(length);
        self
    }

    pub(crate) fn exth(&mut self, kind: u32, data: &[u8]) -> &mut Self {
        self.exth.push((kind, data.to_vec()));
        self
    }

    pub(crate) fn huff_records(&mut self, records: Vec<Vec<u8>>) -> &mut Self {
        self.huff_records = records;
        self
    }

    pub(crate) fn image(&mut self, bytes: &[u8]) -> &mut Self {
        self.images.push(bytes.to_vec());
        self
    }

    pub(crate) fn trailing_data(&mut self, flags: u32, trailers: Vec<Vec<u8>>) -> &mut Self {
        self.extra_data_flags = flags;
        self.trailers = trailers;
        self
    }

    pub(crate) fn kf8(&mut self, book: MobiTestBuilder) -> &mut Self {
        self.kf8 = Some(Box::new(book));
        self
    }

    pub(crate) fn write(&self, path: &Path) {
        let mut exth = self.exth.clone();
        let mut primary = self.build_records(&exth);
        if let Some(kf8) = &self.kf8 {
            let boundary = primary.len() as u32;
            exth.push((121, boundary.to_be_bytes().to_vec()));
            primary = self.build_records(&exth);
            primary.push(b"BOUNDARY".to_vec());
            primary.extend(kf8.build_records(&kf8.exth));
        }
        write_pdb(path, &self.name, &primary);
    }

    fn build_records(&self, exth: &[(u32, Vec<u8>)]) -> Vec<Vec<u8>> {
        let mut record0 = vec![0; 244];
        record0[0..2].copy_from_slice(&self.compression.to_be_bytes());
        let text_length = self
            .text_length
            .unwrap_or_else(|| self.text_records.iter().map(Vec::len).sum::<usize>() as u32);
        record0[4..8].copy_from_slice(&text_length.to_be_bytes());
        record0[8..10].copy_from_slice(&(self.text_records.len() as u16).to_be_bytes());
        record0[10..12].copy_from_slice(&4096u16.to_be_bytes());
        record0[12..14].copy_from_slice(&self.encryption.to_be_bytes());
        record0[16..20].copy_from_slice(b"MOBI");
        record0[20..24].copy_from_slice(&228u32.to_be_bytes());
        let mobi_type: u32 = if self.version >= 8 { 248 } else { 2 };
        record0[24..28].copy_from_slice(&mobi_type.to_be_bytes());
        record0[28..32].copy_from_slice(&self.encoding.to_be_bytes());
        record0[36..40].copy_from_slice(&self.version.to_be_bytes());
        record0[92..96].copy_from_slice(&self.locale.to_be_bytes());
        let first_image = if self.images.is_empty() {
            u32::MAX
        } else {
            1 + self.text_records.len() as u32 + self.huff_records.len() as u32
        };
        record0[108..112].copy_from_slice(&first_image.to_be_bytes());
        let huff_offset = if self.huff_records.is_empty() {
            u32::MAX
        } else {
            1 + self.text_records.len() as u32
        };
        record0[112..116].copy_from_slice(&huff_offset.to_be_bytes());
        record0[116..120].copy_from_slice(&(self.huff_records.len() as u32).to_be_bytes());
        record0[128..132]
            .copy_from_slice(&(if exth.is_empty() { 0u32 } else { 0x40 }).to_be_bytes());
        record0[242..244].copy_from_slice(&(self.extra_data_flags as u16).to_be_bytes());

        if !exth.is_empty() {
            let length = 12 + exth.iter().map(|(_, data)| 8 + data.len()).sum::<usize>();
            record0.extend_from_slice(b"EXTH");
            record0.extend_from_slice(&(length as u32).to_be_bytes());
            record0.extend_from_slice(&(exth.len() as u32).to_be_bytes());
            for (kind, data) in exth {
                record0.extend_from_slice(&kind.to_be_bytes());
                record0.extend_from_slice(&((8 + data.len()) as u32).to_be_bytes());
                record0.extend_from_slice(data);
            }
            while !record0.len().is_multiple_of(4) {
                record0.push(0);
            }
        }
        if let Some(fullname) = &self.fullname {
            let offset = record0.len() as u32;
            record0[84..88].copy_from_slice(&offset.to_be_bytes());
            record0[88..92].copy_from_slice(&(fullname.len() as u32).to_be_bytes());
            record0.extend_from_slice(fullname);
        }

        let mut records = vec![record0];
        for (index, text) in self.text_records.iter().enumerate() {
            let mut record = if self.compression == 2 {
                palmdoc_compress(text)
            } else {
                text.clone()
            };
            if let Some(trailer) = self.trailers.get(index) {
                record.extend_from_slice(trailer);
            }
            records.push(record);
        }
        records.extend(self.huff_records.iter().cloned());
        records.extend(self.images.iter().cloned());
        records
    }
}

pub(crate) fn palmdoc_compress(input: &[u8]) -> Vec<u8> {
    let mut compressed = Vec::with_capacity(input.len() + input.len().div_ceil(8));
    for chunk in input.chunks(8) {
        compressed.push(chunk.len() as u8);
        compressed.extend_from_slice(chunk);
    }
    compressed
}

fn write_pdb(path: &Path, name: &[u8], records: &[Vec<u8>]) {
    let table_end = 78 + records.len() * 8 + 2;
    let mut bytes = vec![0; 78];
    let name_len = name.len().min(31);
    bytes[..name_len].copy_from_slice(&name[..name_len]);
    bytes[60..68].copy_from_slice(b"BOOKMOBI");
    bytes[76..78].copy_from_slice(&(records.len() as u16).to_be_bytes());
    let mut offset = table_end;
    for record in records {
        bytes.extend_from_slice(&(offset as u32).to_be_bytes());
        bytes.extend_from_slice(&[0; 4]);
        offset += record.len();
    }
    bytes.extend_from_slice(&[0; 2]);
    for record in records {
        bytes.extend_from_slice(record);
    }
    std::fs::write(path, bytes).unwrap();
}
