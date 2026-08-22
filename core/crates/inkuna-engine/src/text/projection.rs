//! Canonical text projection: THE text stream of one spine resource.
//!
//! Exact rules: text nodes of rendered elements in document order;
//! `display: none` subtrees excluded; `rt` subtree text excluded (ruby
//! annotation is display-only) while ruby base text is included; runs of
//! Unicode whitespace collapse to one ASCII space; a block-element
//! boundary emits exactly one `\n` (the block set matches the `BLOCK`
//! list in inkuna-content's `text.rs`, the search-corpus extractor);
//! `br` emits `\n`; each block's leading/trailing whitespace is trimmed;
//! never a double `\n`; soft hyphens (U+00AD) are dropped; no generated
//! content, no list markers. All offsets are Unicode scalar counts.
//!
//! INVARIANT: input is publisher CSS + UA defaults only — reader
//! settings never reach this pass, so the stream is stable across every
//! appearance change (see the module doc in `mod.rs`).

use std::collections::HashMap;

use crate::dom::{ElementName, NodeId, NodeKind};
use crate::style::StyledDocument;

/// The canonical stream of one resource plus the maps back into the DOM.
#[derive(Debug)]
pub struct Projection {
    /// THE canonical text.
    pub text: String,
    /// `text.chars().count()`, precomputed.
    pub char_len: u64,
    /// Every contributing text node, in document order.
    pub spans: Vec<TextSpan>,
    /// Each `id` → the offset of the first projected char at-or-after
    /// the element's start (`char_len` at end of document), doc order.
    pub anchors: Vec<(String, u64)>,
    /// Propagated from the DOM parse: the stream is a prefix.
    pub truncated: bool,
}

/// One text node's contribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSpan {
    pub node: NodeId,
    /// Char range into [`Projection::text`]. Separator whitespace between
    /// two nodes belongs to neither span.
    pub char_range: std::ops::Range<u64>,
    /// Offset inside the node's own collapsed text where this span
    /// begins. A text node contributes one contiguous span today, so
    /// this is 0; the field is the contract for any pass that ever
    /// splits one.
    pub node_char_start: u64,
}

/// Projects the styled document onto its canonical stream. Infallible;
/// truncation propagates from the DOM flag.
pub fn project(styled: &StyledDocument<'_>) -> Projection {
    // Per-node id lists, so the walk pins each id in traversal (=
    // document) order without rescanning the anchor list per node.
    let mut ids_by_node: HashMap<u32, Vec<String>> = HashMap::new();
    for (id, node) in &styled.doc.anchors {
        // Owned copies: the projection outlives the borrow of the doc.
        ids_by_node.entry(node.0).or_default().push(id.clone());
    }
    let mut p = Projector {
        styled,
        ids_by_node,
        out: String::new(),
        out_chars: 0,
        pending_break: false,
        pending_space: false,
        at_line_start: true,
        pending_anchors: Vec::new(),
        anchors: Vec::new(),
        spans: Vec::new(),
    };
    p.walk(styled.doc.root);
    let Projector {
        out,
        out_chars,
        mut anchors,
        pending_anchors,
        spans,
        ..
    } = p;
    // Ids whose elements projected nothing more anchor at the end.
    anchors.extend(
        pending_anchors
            .into_iter()
            .map(|id| (id, out_chars)),
    );
    Projection {
        text: out,
        char_len: out_chars,
        spans,
        anchors,
        truncated: styled.doc.truncated,
    }
}

struct Projector<'a, 'd> {
    styled: &'a StyledDocument<'d>,
    /// Ids keyed by the node that carries them, drained as visited.
    ids_by_node: HashMap<u32, Vec<String>>,
    out: String,
    out_chars: u64,
    /// A block boundary or `br` awaits its `\n`; materialized before the
    /// next projected char, so the stream never ends in one and never
    /// doubles one.
    pending_break: bool,
    /// Collapsed whitespace awaits its single space.
    pending_space: bool,
    /// Start of output or just after `\n`: suppress leading spaces.
    at_line_start: bool,
    /// Ids seen whose first projected char has not arrived yet.
    pending_anchors: Vec<String>,
    anchors: Vec<(String, u64)>,
    spans: Vec<TextSpan>,
}

impl Projector<'_, '_> {
    fn walk(&mut self, id: NodeId) {
        let doc = self.styled.doc;
        match &doc.node(id).kind {
            NodeKind::Text(text) => self.text_node(id, text),
            NodeKind::Element(data) => {
                self.collect_ids(id);
                if self.styled.styles[id.0 as usize].display_none
                    || data.name == ElementName::Rt
                {
                    // Excluded subtree: its ids still anchor (at the
                    // next projected char), its text never projects.
                    self.collect_subtree_ids(id);
                    return;
                }
                let block = is_block(&data.name);
                if block || data.name == ElementName::Br {
                    self.break_line();
                }
                for &child in &doc.node(id).children {
                    self.walk(child);
                }
                if block {
                    self.break_line();
                }
            }
        }
    }

    /// Emits one text node, collapsing whitespace, dropping soft
    /// hyphens, and recording its span.
    fn text_node(&mut self, id: NodeId, text: &str) {
        let mut span_start: Option<u64> = None;
        for ch in text.chars() {
            if ch == '\u{AD}' {
                continue; // soft hyphen: never part of the stream
            }
            if ch.is_whitespace() {
                self.pending_space = true;
                continue;
            }
            self.separators();
            self.resolve_anchors();
            span_start.get_or_insert(self.out_chars);
            self.out.push(ch);
            self.out_chars += 1;
            self.at_line_start = false;
        }
        if let Some(start) = span_start {
            self.spans.push(TextSpan {
                node: id,
                char_range: start..self.out_chars,
                node_char_start: 0,
            });
        }
    }

    /// Materializes pending separators before a projected char: a break
    /// beats a space, a leading space is trimmed.
    fn separators(&mut self) {
        if self.pending_break {
            if !self.out.is_empty() {
                self.out.push('\n');
                self.out_chars += 1;
            }
            self.at_line_start = true;
        } else if self.pending_space && !self.at_line_start {
            self.out.push(' ');
            self.out_chars += 1;
        }
        self.pending_break = false;
        self.pending_space = false;
    }

    /// A block boundary (or `br`): one `\n` before the next char, and
    /// any pending space dies with the block edge (trimming).
    fn break_line(&mut self) {
        self.pending_break = true;
        self.pending_space = false;
    }

    /// Pins every waiting id to the char about to be emitted.
    fn resolve_anchors(&mut self) {
        for id in self.pending_anchors.drain(..) {
            self.anchors.push((id, self.out_chars));
        }
    }

    /// Queues the ids attached to this node (the DOM keeps flattened
    /// elements' ids on their retained ancestor, so this is map-driven,
    /// not `data.id`-driven).
    fn collect_ids(&mut self, id: NodeId) {
        if let Some(ids) = self.ids_by_node.remove(&id.0) {
            self.pending_anchors.extend(ids);
        }
    }

    /// Queues ids inside an excluded subtree, in document order.
    fn collect_subtree_ids(&mut self, id: NodeId) {
        let doc = self.styled.doc;
        for &child in &doc.node(id).children {
            self.collect_ids(child);
            self.collect_subtree_ids(child);
        }
    }
}

/// The block set — kept identical to the `BLOCK` const in
/// inkuna-content's `text.rs`, because that extractor and this
/// projection must segment text the same way.
fn is_block(name: &ElementName) -> bool {
    matches!(
        name,
        ElementName::P
            | ElementName::Div
            | ElementName::H1
            | ElementName::H2
            | ElementName::H3
            | ElementName::H4
            | ElementName::H5
            | ElementName::H6
            | ElementName::Li
            | ElementName::Blockquote
            | ElementName::Section
            | ElementName::Article
            | ElementName::Tr
            | ElementName::Caption
            | ElementName::Figcaption
            | ElementName::Dt
            | ElementName::Dd
            | ElementName::Pre
    )
}

#[cfg(test)]
#[path = "projection_tests.rs"]
mod tests;
