//! `META-INF/container.xml`: the one fixed path in an EPUB, pointing at the
//! OPF package document.

use quick_xml::events::Event;
use quick_xml::Reader;

use super::xml::attr_value;
use crate::CoreError;

pub(super) fn rootfile_path(container_xml: &str) -> Result<String, CoreError> {
    let mut reader = Reader::from_str(container_xml);
    // No end-name validation: it would grow a stack of open element names
    // on a crafted deeply-nested document (see the note in `parse_opf`).
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == b"rootfile" => {
                if let Some(path) = attr_value(&e, b"full-path") {
                    return Ok(path);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(CoreError::InvalidPublication(e.to_string())),
            _ => {}
        }
        buf.clear();
    }
    Err(CoreError::InvalidPublication("no rootfile in container.xml".into()))
}
