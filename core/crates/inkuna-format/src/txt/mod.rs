//! Plain-text decoding and EPUB normalization.

mod chapters;
mod charset;
mod convert;
mod paragraphs;

pub use charset::bomless_utf16;
pub use convert::convert_to_epub;
