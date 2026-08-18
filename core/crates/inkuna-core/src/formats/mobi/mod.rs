//! Bounded, clean-room parsing and EPUB conversion of DRM-free Mobipocket
//! Palm databases.

mod book;
mod convert;
mod convert8;
mod entities;
mod header;
mod huffcdic;
mod indx;
mod kf8;
mod markup;
mod palmdoc;
mod pdb;
mod sanitize;
mod scan;

pub(crate) use book::MobiBook;
pub(crate) use convert::convert_to_epub;
