//! The compact arena tree one parsed resource yields. Nodes live in one
//! `Vec`; identity is an index, so the whole tree is cache-dense and
//! droppable in one free.

/// Index of a node in [`Document::nodes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

/// What one arena node is.
#[derive(Debug)]
pub enum NodeKind {
    Element(ElementData),
    /// Character data, entity-resolved, verbatim (whitespace collapsing
    /// is the projection's job, not the parser's).
    Text(String),
}

/// The attributes layout honors, captured verbatim at parse time. Every
/// value is budgeted by [`super::MAX_ATTR_BYTES`].
#[derive(Debug)]
pub struct ElementData {
    /// Interned element name; `i` maps to `Em` and `b` to `Strong` here.
    pub name: ElementName,
    /// The `class` attribute, verbatim (space-separated list).
    pub class: Option<Box<str>>,
    /// The `id` attribute.
    pub id: Option<Box<str>>,
    /// `lang` or `xml:lang`.
    pub lang: Option<Box<str>>,
    /// The inline `style` attribute.
    pub style_attr: Option<Box<str>>,
    /// `a href` / `img src` / `image href`, verbatim.
    pub href: Option<Box<str>>,
    /// The `dir` attribute: `Some(true)` for `rtl`, `Some(false)` for
    /// `ltr`, `None` when absent or unrecognized.
    pub dir_rtl: Option<bool>,
}

/// One node: kind plus tree links.
#[derive(Debug)]
pub struct Node {
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

/// One parsed resource.
#[derive(Debug)]
pub struct Document {
    pub nodes: Vec<Node>,
    /// The root element (the `<html>` element in a well-formed resource).
    pub root: NodeId,
    /// Every `id` attribute, in document order.
    pub anchors: Vec<(String, NodeId)>,
    /// Stylesheet sources in document order: `<link rel="stylesheet">`
    /// hrefs and `<style>` bodies.
    pub stylesheets: Vec<StylesheetSource>,
    /// A parse budget tripped and the tree is a prefix of the resource.
    pub truncated: bool,
}

/// Where a stylesheet came from.
#[derive(Debug, PartialEq, Eq)]
pub enum StylesheetSource {
    /// A `<link rel="stylesheet">` href, verbatim (resolved at load time).
    Linked(String),
    /// The text of a `<style>` element.
    Inline(String),
}

/// The semantically honored element names; everything else is `Other`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementName {
    Html,
    Body,
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    P,
    Div,
    Span,
    Em,
    Strong,
    Blockquote,
    Pre,
    Code,
    Ol,
    Ul,
    Li,
    Ruby,
    Rt,
    Rb,
    Img,
    Image,
    Table,
    Tr,
    Td,
    Th,
    Caption,
    Br,
    Hr,
    A,
    Section,
    Article,
    Figure,
    Figcaption,
    Dt,
    Dd,
    Other(Box<str>),
}

impl ElementName {
    /// Interns a lowercased tag name; `i` → `Em`, `b` → `Strong`. The one
    /// mapping both the parser and selector matching share, so a CSS
    /// `i {}` rule matches the `Em` the parser interned.
    pub fn from_tag(tag: &str) -> Self {
        match tag.as_bytes() {
            b"html" => Self::Html,
            b"body" => Self::Body,
            b"h1" => Self::H1,
            b"h2" => Self::H2,
            b"h3" => Self::H3,
            b"h4" => Self::H4,
            b"h5" => Self::H5,
            b"h6" => Self::H6,
            b"p" => Self::P,
            b"div" => Self::Div,
            b"span" => Self::Span,
            b"em" | b"i" => Self::Em,
            b"strong" | b"b" => Self::Strong,
            b"blockquote" => Self::Blockquote,
            b"pre" => Self::Pre,
            b"code" => Self::Code,
            b"ol" => Self::Ol,
            b"ul" => Self::Ul,
            b"li" => Self::Li,
            b"ruby" => Self::Ruby,
            b"rt" => Self::Rt,
            b"rb" => Self::Rb,
            b"img" => Self::Img,
            b"image" => Self::Image,
            b"table" => Self::Table,
            b"tr" => Self::Tr,
            b"td" => Self::Td,
            b"th" => Self::Th,
            b"caption" => Self::Caption,
            b"br" => Self::Br,
            b"hr" => Self::Hr,
            b"a" => Self::A,
            b"section" => Self::Section,
            b"article" => Self::Article,
            b"figure" => Self::Figure,
            b"figcaption" => Self::Figcaption,
            b"dt" => Self::Dt,
            b"dd" => Self::Dd,
            _ => Self::Other(tag.into()),
        }
    }
}

impl Document {
    /// The element data of `id`, `None` for text nodes.
    pub fn element(&self, id: NodeId) -> Option<&ElementData> {
        match &self.nodes[id.0 as usize].kind {
            NodeKind::Element(data) => Some(data),
            NodeKind::Text(_) => None,
        }
    }

    /// The node behind `id`.
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }
}
