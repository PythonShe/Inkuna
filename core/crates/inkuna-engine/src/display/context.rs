//! The per-chapter display context: everything page emission consumes,
//! built once per layout pass and reused for every page — the link
//! stretches with normalized targets, the char→byte map, and the face
//! metrics cache.

use std::ops::Range;

use crate::dom::{ElementName, NodeId};
use crate::fixed::Fx;
use crate::fonts::FontRegistry;
use crate::paginate::FxSize;
use crate::style::StyledDocument;
use crate::text::Projection;

/// One link's canonical char stretch, precomputed per chapter.
pub(super) struct LinkSpan {
    pub range: Range<u64>,
    pub target: String,
}

/// The per-chapter inputs display emission consumes.
pub struct DisplayContext<'a> {
    pub(super) generation: u64,
    pub(super) styled: &'a StyledDocument<'a>,
    pub(super) projection: &'a Projection,
    pub(super) fonts: &'a FontRegistry,
    pub(super) viewport: FxSize,
    /// Byte offset of each projection char, plus the end.
    pub(super) byte_of_char: Vec<usize>,
    /// Link stretches, ascending and disjoint.
    pub(super) links: Vec<LinkSpan>,
}

impl<'a> DisplayContext<'a> {
    pub fn new(
        generation: u64,
        styled: &'a StyledDocument<'a>,
        projection: &'a Projection,
        fonts: &'a FontRegistry,
        viewport: FxSize,
        resource_path: &str,
    ) -> DisplayContext<'a> {
        let mut byte_of_char: Vec<usize> =
            projection.text.char_indices().map(|(b, _)| b).collect();
        byte_of_char.push(projection.text.len());
        DisplayContext {
            generation,
            styled,
            projection,
            fonts,
            viewport,
            byte_of_char,
            links: link_spans(styled, projection, resource_path),
        }
    }

    /// The link covering canonical char `c`, if any.
    pub(super) fn link_at(&self, c: u64) -> Option<usize> {
        let idx = self.links.partition_point(|l| l.range.start <= c);
        let candidate = idx.checked_sub(1)?;
        (self.links[candidate].range.end > c).then_some(candidate)
    }
}

/// Every link stretch of the chapter: consecutive projection spans
/// sharing an `a`-with-href ancestor merge into one range (separator
/// chars between them included), in document order.
fn link_spans(
    styled: &StyledDocument<'_>,
    projection: &Projection,
    resource_path: &str,
) -> Vec<LinkSpan> {
    let doc = styled.doc;
    let mut out: Vec<(NodeId, LinkSpan)> = Vec::new();
    for span in &projection.spans {
        let mut cur = doc.node(span.node).parent;
        let mut found = None;
        while let Some(id) = cur {
            if let Some(el) = doc.element(id) {
                if el.name == ElementName::A {
                    if let Some(href) = &el.href {
                        found = Some((id, href.to_string()));
                        break;
                    }
                }
            }
            cur = doc.node(id).parent;
        }
        let Some((node, href)) = found else { continue };
        match out.last_mut() {
            Some((last_node, link)) if *last_node == node => {
                link.range.end = span.char_range.end;
            }
            _ => out.push((
                node,
                LinkSpan {
                    range: span.char_range.clone(),
                    target: normalize_target(resource_path, &href),
                },
            )),
        }
    }
    out.into_iter().map(|(_, l)| l).collect()
}

/// External `http(s):`/`mailto:` targets stay verbatim; everything else
/// resolves package-root-relative against the chapter's own path,
/// keeping the fragment.
fn normalize_target(resource_path: &str, href: &str) -> String {
    let lower = href.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
    {
        return href.to_string();
    }
    inkuna_content::resolve_relative(resource_path, href)
}

/// Per-face vertical metrics `(ascender, descender-as-positive, upem)`,
/// cached per page build.
#[derive(Default)]
pub(super) struct FaceMetrics(Vec<(u32, i32, i32, u16)>);

impl FaceMetrics {
    pub(super) fn get(&mut self, fonts: &FontRegistry, font_id: u32) -> (i32, i32, u16) {
        if let Some(&(_, asc, desc, upem)) = self.0.iter().find(|&&(id, ..)| id == font_id) {
            return (asc, desc, upem);
        }
        let face = fonts.face(font_id);
        let (asc, desc) =
            match rustybuzz::ttf_parser::Face::parse(&face.data, face.collection_index) {
                Ok(parsed) => (
                    i32::from(parsed.ascender()),
                    i32::from(parsed.descender()).saturating_neg(),
                ),
                Err(_) => (i32::from(face.upem) * 3 / 4, i32::from(face.upem) / 4),
            };
        self.0.push((font_id, asc, desc, face.upem));
        (asc, desc, face.upem)
    }

    /// Scaled descent of a face at `size`.
    pub(super) fn descent(&mut self, fonts: &FontRegistry, font_id: u32, size: Fx) -> Fx {
        let (_, desc, upem) = self.get(fonts, font_id);
        Fx::scale_font_units(desc, size, upem)
    }

    /// Scaled ascent of a face at `size`.
    pub(super) fn ascent(&mut self, fonts: &FontRegistry, font_id: u32, size: Fx) -> Fx {
        let (asc, _, upem) = self.get(fonts, font_id);
        Fx::scale_font_units(asc, size, upem)
    }
}
