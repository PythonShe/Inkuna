//! Plain-text decoding and EPUB normalization.

mod chapters;
mod charset;
mod convert;
mod paragraphs;

pub(crate) use charset::bomless_utf16;
pub(crate) use convert::convert_to_epub;
