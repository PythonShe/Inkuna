use std::path::Path;

pub(crate) struct IndxEntryFixture {
    label: Vec<u8>,
    tags: Vec<(u8, Vec<u32>)>,
}

pub(crate) struct Kf8FileFixture {
    skeleton: Vec<u8>,
    fragments: Vec<(usize, Vec<u8>)>,
}

impl Kf8FileFixture {
    pub(crate) fn new(skeleton: &[u8]) -> Self {
        Self {
            skeleton: skeleton.to_vec(),
            fragments: Vec::new(),
        }
    }

    pub(crate) fn fragment(mut self, insert: usize, bytes: &[u8]) -> Self {
        self.fragments.push((insert, bytes.to_vec()));
        self
    }
}

pub(crate) struct Kf8NcxFixture {
    label: String,
    file: usize,
    depth: u32,
}

impl Kf8NcxFixture {
    /// `file` must be a valid index into the `kf8_files` list passed to the
    /// builder: `build_kf8_bundle` resolves it via `skeleton_starts[entry.file]`
    /// and panics on an out-of-range value.
    pub(crate) fn new(label: &str, file: usize, depth: u32) -> Self {
        Self {
            label: label.into(),
            file,
            depth,
        }
    }
}

struct Kf8Bundle {
    text_records: Vec<Vec<u8>>,
    records: Vec<Vec<u8>>,
    fdst: u32,
    flow_count: u32,
    fragment_index: u32,
    skeleton_index: u32,
    ncx_index: Option<u32>,
}

impl IndxEntryFixture {
    pub(crate) fn new(label: &[u8]) -> Self {
        Self {
            label: label.to_vec(),
            tags: Vec::new(),
        }
    }

    pub(crate) fn tag(mut self, tag: u8, values: &[u32]) -> Self {
        self.tags.push((tag, values.to_vec()));
        self
    }
}

pub(crate) fn build_indx_records(
    tag_table: &[(u8, u8, u8, u8)],
    entries: &[IndxEntryFixture],
) -> Vec<Vec<u8>> {
    let control_bytes = 1 + tag_table.iter().filter(|entry| entry.3 == 1).count();
    // Every real TAGX control-byte group ends with the documented all-zero
    // entry whose fourth byte is 1. Callers only describe separators between
    // groups; the final terminator is emitted here so fixtures cannot drift
    // from the parser's on-disk layout.
    let tagx_length = 12 + (tag_table.len() + 1) * 4;
    let header_length = 56usize;
    let mut meta = vec![0; header_length];
    meta[0..4].copy_from_slice(b"INDX");
    meta[4..8].copy_from_slice(&(header_length as u32).to_be_bytes());
    meta[24..28].copy_from_slice(&1u32.to_be_bytes());
    meta[28..32].copy_from_slice(&65001u32.to_be_bytes());
    meta[36..40].copy_from_slice(&(entries.len() as u32).to_be_bytes());
    meta.extend_from_slice(b"TAGX");
    meta.extend_from_slice(&(tagx_length as u32).to_be_bytes());
    meta.extend_from_slice(&(control_bytes as u32).to_be_bytes());
    for entry in tag_table {
        meta.extend_from_slice(&[entry.0, entry.1, entry.2, entry.3]);
    }
    meta.extend_from_slice(&[0, 0, 0, 1]);

    let data_header_length = 32usize;
    let mut data = vec![0; data_header_length];
    data[0..4].copy_from_slice(b"INDX");
    data[4..8].copy_from_slice(&(data_header_length as u32).to_be_bytes());
    data[24..28].copy_from_slice(&(entries.len() as u32).to_be_bytes());
    let mut offsets = Vec::with_capacity(entries.len());
    for entry in entries {
        offsets.push(u16::try_from(data.len()).unwrap());
        data.push(u8::try_from(entry.label.len()).unwrap());
        data.extend_from_slice(&entry.label);
        let controls_start = data.len();
        data.resize(controls_start + control_bytes, 0);
        let mut control_index = 0usize;
        for &(tag, values_per_entry, mask, end) in tag_table {
            if end == 1 {
                control_index += 1;
                continue;
            }
            let Some((_, values)) = entry.tags.iter().find(|(candidate, _)| *candidate == tag)
            else {
                continue;
            };
            let count = values.len() / usize::from(values_per_entry);
            // Control-byte rule, the exact mirror of `parse_entry` in
            // formats/azw3/indx.rs: a single-bit mask can only encode
            // "present once" (the mask value itself); a multi-bit mask stores
            // the occurrence count shifted into the mask, with the
            // all-bits-set value announcing a varint byte-length budget
            // followed by that many bytes of varint values.
            if mask.count_ones() == 1 {
                assert_eq!(
                    count, 1,
                    "single-bit TAGX masks encode exactly one occurrence"
                );
                data[controls_start + control_index] |= mask;
                for value in values {
                    push_varuint(&mut data, *value);
                }
            } else {
                let shift = mask.trailing_zeros();
                let inline_max = usize::from(mask >> shift);
                if count < inline_max {
                    data[controls_start + control_index] |= (count as u8) << shift;
                    for value in values {
                        push_varuint(&mut data, *value);
                    }
                } else {
                    data[controls_start + control_index] |= mask;
                    let mut encoded = Vec::new();
                    for value in values {
                        push_varuint(&mut encoded, *value);
                    }
                    push_varuint(&mut data, encoded.len() as u32);
                    data.extend_from_slice(&encoded);
                }
            }
        }
    }
    let idxt_start = data.len();
    data[20..24].copy_from_slice(&(idxt_start as u32).to_be_bytes());
    data.extend_from_slice(b"IDXT");
    for offset in offsets {
        data.extend_from_slice(&offset.to_be_bytes());
    }
    vec![meta, data]
}

fn push_varuint(output: &mut Vec<u8>, mut value: u32) {
    let mut encoded = [0u8; 5];
    let mut cursor = encoded.len();
    loop {
        cursor -= 1;
        encoded[cursor] = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            break;
        }
    }
    encoded[encoded.len() - 1] |= 0x80;
    output.extend_from_slice(&encoded[cursor..]);
}

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
    kf8_files: Option<Vec<Kf8FileFixture>>,
    css_flows: Vec<Vec<u8>>,
    ncx: Option<Vec<Kf8NcxFixture>>,
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
            kf8_files: None,
            css_flows: Vec::new(),
            ncx: None,
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

    pub(crate) fn html(&mut self, html: &[u8]) -> &mut Self {
        self.text_records = vec![html.to_vec()];
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

    pub(crate) fn kf8_files(&mut self, files: Vec<Kf8FileFixture>) -> &mut Self {
        self.kf8_files = Some(files);
        self
    }

    pub(crate) fn css_flow(&mut self, css: &[u8]) -> &mut Self {
        self.css_flows.push(css.to_vec());
        self
    }

    pub(crate) fn ncx(&mut self, entries: Vec<Kf8NcxFixture>) -> &mut Self {
        self.ncx = Some(entries);
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
        let mobi_length = if self.version >= 8 {
            264usize
        } else {
            228usize
        };
        let mut record0 = vec![0; 16 + mobi_length];
        let base_text_records = self.text_records.clone();
        let kf8_base = 1 + base_text_records.len() + self.huff_records.len() + self.images.len();
        let kf8 = self
            .kf8_files
            .as_ref()
            .map(|files| self.build_kf8_bundle(files, kf8_base as u32));
        let text_records = kf8.as_ref().map_or(base_text_records.as_slice(), |bundle| {
            bundle.text_records.as_slice()
        });
        record0[0..2].copy_from_slice(&self.compression.to_be_bytes());
        let text_length = self
            .text_length
            .unwrap_or_else(|| text_records.iter().map(Vec::len).sum::<usize>() as u32);
        record0[4..8].copy_from_slice(&text_length.to_be_bytes());
        record0[8..10].copy_from_slice(&(text_records.len() as u16).to_be_bytes());
        record0[10..12].copy_from_slice(&4096u16.to_be_bytes());
        record0[12..14].copy_from_slice(&self.encryption.to_be_bytes());
        record0[16..20].copy_from_slice(b"MOBI");
        record0[20..24].copy_from_slice(&(mobi_length as u32).to_be_bytes());
        let mobi_type: u32 = if self.version >= 8 { 248 } else { 2 };
        record0[24..28].copy_from_slice(&mobi_type.to_be_bytes());
        record0[28..32].copy_from_slice(&self.encoding.to_be_bytes());
        record0[36..40].copy_from_slice(&self.version.to_be_bytes());
        record0[92..96].copy_from_slice(&self.locale.to_be_bytes());
        let first_image = if self.images.is_empty() {
            u32::MAX
        } else {
            1 + text_records.len() as u32 + self.huff_records.len() as u32
        };
        record0[108..112].copy_from_slice(&first_image.to_be_bytes());
        let huff_offset = if self.huff_records.is_empty() {
            u32::MAX
        } else {
            1 + text_records.len() as u32
        };
        record0[112..116].copy_from_slice(&huff_offset.to_be_bytes());
        record0[116..120].copy_from_slice(&(self.huff_records.len() as u32).to_be_bytes());
        record0[128..132]
            .copy_from_slice(&(if exth.is_empty() { 0u32 } else { 0x40 }).to_be_bytes());
        record0[242..244].copy_from_slice(&(self.extra_data_flags as u16).to_be_bytes());
        if self.version >= 8 {
            for offset in [244usize, 248, 252, 256, 260] {
                record0[offset..offset + 4].copy_from_slice(&u32::MAX.to_be_bytes());
            }
            if let Some(bundle) = &kf8 {
                record0[192..196].copy_from_slice(&bundle.fdst.to_be_bytes());
                record0[196..200].copy_from_slice(&bundle.flow_count.to_be_bytes());
                record0[248..252].copy_from_slice(&bundle.fragment_index.to_be_bytes());
                record0[252..256].copy_from_slice(&bundle.skeleton_index.to_be_bytes());
                if let Some(ncx) = bundle.ncx_index {
                    record0[244..248].copy_from_slice(&ncx.to_be_bytes());
                }
            }
        }

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
        for (index, text) in text_records.iter().enumerate() {
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
        if let Some(bundle) = kf8 {
            records.extend(bundle.records);
        }
        records
    }

    fn build_kf8_bundle(&self, files: &[Kf8FileFixture], first_record: u32) -> Kf8Bundle {
        let mut flow0 = Vec::new();
        let mut skeleton_entries = Vec::new();
        let mut fragment_entries = Vec::new();
        let mut skeleton_starts = Vec::new();
        for file in files {
            let start = flow0.len() as u32;
            skeleton_starts.push(start);
            flow0.extend_from_slice(&file.skeleton);
            skeleton_entries.push(
                IndxEntryFixture::new(b"SKEL")
                    .tag(1, &[file.fragments.len() as u32])
                    .tag(6, &[start, file.skeleton.len() as u32]),
            );
        }
        for (file_number, file) in files.iter().enumerate() {
            for (sequence, (insert, fragment)) in file.fragments.iter().enumerate() {
                let start = flow0.len() as u32;
                flow0.extend_from_slice(fragment);
                fragment_entries.push(
                    IndxEntryFixture::new(insert.to_string().as_bytes())
                        .tag(3, &[file_number as u32])
                        .tag(4, &[sequence as u32])
                        .tag(6, &[start, fragment.len() as u32]),
                );
            }
        }
        let mut raw = flow0;
        let mut pairs = vec![(0u32, raw.len() as u32)];
        for css in &self.css_flows {
            let start = raw.len() as u32;
            raw.extend_from_slice(css);
            pairs.push((start, raw.len() as u32));
        }
        let mut fdst = Vec::from(b"FDST".as_slice());
        fdst.extend_from_slice(&(12 + pairs.len() as u32 * 8).to_be_bytes());
        fdst.extend_from_slice(&(pairs.len() as u32).to_be_bytes());
        for (start, end) in &pairs {
            fdst.extend_from_slice(&start.to_be_bytes());
            fdst.extend_from_slice(&end.to_be_bytes());
        }

        let fragment = build_indx_records(
            &[(3, 1, 0x01, 0), (4, 1, 0x02, 0), (6, 2, 0x04, 0)],
            &fragment_entries,
        );
        let skeleton = build_indx_records(&[(1, 1, 0x01, 0), (6, 2, 0x02, 0)], &skeleton_entries);
        let fragment_index = first_record + 1;
        let skeleton_index = fragment_index + fragment.len() as u32;
        let mut records = vec![fdst];
        records.extend(fragment);
        records.extend(skeleton);
        let ncx_index = self.ncx.as_ref().map(|entries| {
            let index = first_record + records.len() as u32;
            let fixtures = entries
                .iter()
                .map(|entry| {
                    IndxEntryFixture::new(entry.label.as_bytes())
                        .tag(1, &[skeleton_starts[entry.file]])
                        .tag(4, &[entry.depth])
                })
                .collect::<Vec<_>>();
            records.extend(build_indx_records(
                &[(1, 1, 0x01, 0), (4, 1, 0x02, 0)],
                &fixtures,
            ));
            index
        });
        Kf8Bundle {
            text_records: vec![raw],
            records,
            fdst: first_record,
            flow_count: pairs.len() as u32,
            fragment_index,
            skeleton_index,
            ncx_index,
        }
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
