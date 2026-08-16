//! Publication format, mirroring the core's detection result.

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Format {
    Epub,
    Mobi,
    Azw3,
    Txt,
    Pdf,
    Cbz,
    Cbr,
}

impl From<inkuna_core::Format> for Format {
    fn from(f: inkuna_core::Format) -> Self {
        match f {
            inkuna_core::Format::Epub => Format::Epub,
            inkuna_core::Format::Mobi => Format::Mobi,
            inkuna_core::Format::Azw3 => Format::Azw3,
            inkuna_core::Format::Txt => Format::Txt,
            inkuna_core::Format::Pdf => Format::Pdf,
            inkuna_core::Format::Cbz => Format::Cbz,
            inkuna_core::Format::Cbr => Format::Cbr,
        }
    }
}
