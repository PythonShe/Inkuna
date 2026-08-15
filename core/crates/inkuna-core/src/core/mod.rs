//! Infrastructure shared by every feature: the crate error type, SQLite
//! plumbing, and small filesystem/time leaves. No business logic lives here.

pub(crate) mod db;
pub(crate) mod error;
pub(crate) mod files;
pub(crate) mod time;
