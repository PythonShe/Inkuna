//! Bounded, clean-room parsing of DRM-free Mobipocket Palm databases.

mod book;
mod header;
mod huffcdic;
mod palmdoc;
mod pdb;

#[allow(unused_imports)]
pub(crate) use book::MobiBook;
