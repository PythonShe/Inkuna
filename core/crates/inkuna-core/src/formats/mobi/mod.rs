//! Bounded, clean-room parsing and EPUB conversion of DRM-free Mobipocket
//! Palm databases.

mod book;
mod convert;
mod entities;
mod header;
mod huffcdic;
pub(super) mod markup;
mod palmdoc;
mod pdb;
pub(super) mod sanitize;
mod scan;

pub(crate) use book::MobiBook;
pub(super) use book::MAX_TOTAL_TEXT_BYTES;
pub(crate) use convert::convert_to_epub;
pub(super) use convert::image_type;
