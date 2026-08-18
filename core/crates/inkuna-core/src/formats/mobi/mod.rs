//! Bounded, clean-room parsing and EPUB conversion of DRM-free Mobipocket
//! Palm databases.

mod book;
mod convert;
mod entities;
mod header;
mod huffcdic;
mod markup;
mod palmdoc;
mod pdb;
mod scan;
mod sanitize;

pub(crate) use book::MobiBook;
pub(crate) use convert::convert_to_epub;
