//! Bounded, clean-room KF8 (AZW3) structure reading — INDX/TAGX indexes,
//! skeleton/fragment assembly — and conversion into the normalized EPUB 3
//! import format. The Palm container layer lives in [`super::mobi`].

mod convert;
mod indx;
mod kf8;

pub use convert::convert_to_epub;
