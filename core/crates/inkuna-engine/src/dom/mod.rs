//! The engine's document model: a compact arena tree parsed leniently from
//! one XHTML spine resource, plus the stylesheet sources found on the way.
//! Budget-guarded — a hostile resource truncates, it never exhausts.

mod arena;
mod parse;

#[cfg(test)]
mod tests;

pub use arena::{Document, ElementData, ElementName, Node, NodeId, NodeKind, StylesheetSource};
pub use parse::{
    parse, MAX_ATTR_BYTES, MAX_DEPTH, MAX_DOM_NODES, MAX_STYLESHEET_BYTES, MAX_TEXT_BYTES,
};
