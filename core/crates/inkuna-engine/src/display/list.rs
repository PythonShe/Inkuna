//! The engine-native display-list records and their emission from one
//! [`LaidPage`]. The FFI mirrors these types 1:1 in M6; the shells draw
//! glyph runs verbatim (Core Text / `Canvas.drawGlyphs`) and never infer
//! colors — every color role is assigned here.
//!
//! Geometry contract: positions and rects are page coordinates in layout
//! points at 1× (y grows downward). Each glyph position is the glyph's
//! horizontal-baseline origin; `SidewaysRotated` runs are drawn rotated
//! 90° clockwise about each glyph's emitted position.

use crate::fixed::Fx;
use crate::fonts::FontRegistry;
use crate::paginate::{FxRect, FxSize, LaidPage, PlacedLine};
use crate::shape::RunOrientation;
use crate::style::{StyledDocument, WritingMode};
use crate::text::Projection;

use super::a11y;
use super::maps::{LineGeom, PageMaps};

/// A hairline: 1 pt, for underline decorations.
const HAIRLINE: Fx = Fx(64);

/// The shells' theme-mapped color slots. Core assigns roles; shells
/// resolve them to actual colors per theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorRole {
    Text,
    Secondary,
    Link,
}

/// One drawable glyph sequence: a single face at a single size and
/// color role.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRun {
    pub font_id: u32,
    pub size: f64,
    pub color_role: ColorRole,
    pub glyph_ids: Vec<u16>,
    /// x,y interleaved, len = 2 × glyphs, page coordinates, layout
    /// points at 1×.
    pub positions: Vec<f32>,
    pub orientation: RunOrientation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationKind {
    Rule,
    Underline,
}

/// A rectangle in page coordinates, layout points at 1×.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A filled rect the shell draws in the role's color — roles are
/// core-assigned (`Rule` → `Secondary`; `Underline` → `Link` inside a
/// link region, else `Text`), never shell-inferred.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoration {
    pub kind: DecorationKind,
    pub rect: Rect,
    pub color_role: ColorRole,
}

/// One image at its page frame; `href` is normalized
/// package-root-relative.
#[derive(Debug, Clone, PartialEq)]
pub struct ImagePlacement {
    pub href: String,
    pub rect: Rect,
}

/// A tappable link area. Internal targets are normalized
/// package-root-relative hrefs (fragment kept); external
/// `http(s):`/`mailto:` targets are kept verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkRegion {
    pub rect: Rect,
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A11yRole {
    Body,
    Heading,
    Link,
}

/// One block-level element's accessible text on this page, in logical
/// reading order. Ruby readings are appended parenthetically
/// ("漢字（かんじ）").
#[derive(Debug, Clone, PartialEq)]
pub struct A11yBlock {
    pub text: String,
    pub rect: Rect,
    /// Nearest ancestor `lang`/`xml:lang`, `None` when unset.
    pub lang: Option<String>,
    pub is_link: bool,
    pub role: A11yRole,
}

/// Everything the shell draws for one page. All vectors are in
/// deterministic construction order (visual order within lines, lines
/// in block order).
#[derive(Debug, Clone, PartialEq)]
pub struct PageDisplayList {
    pub generation: u64,
    pub glyph_runs: Vec<GlyphRun>,
    pub images: Vec<ImagePlacement>,
    pub decorations: Vec<Decoration>,
    pub links: Vec<LinkRegion>,
    pub a11y: Vec<A11yBlock>,
}

/// One link's canonical char stretch, precomputed per chapter.
pub(super) struct LinkSpan {
    pub range: std::ops::Range<u64>,
    pub target: String,
}

/// The per-chapter inputs display emission consumes, built once per
/// layout pass and reused for every page.
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
    let mut out: Vec<(crate::dom::NodeId, LinkSpan)> = Vec::new();
    for span in &projection.spans {
        let mut cur = doc.node(span.node).parent;
        let mut found = None;
        while let Some(id) = cur {
            if let Some(el) = doc.element(id) {
                if el.name == crate::dom::ElementName::A {
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
    fn descent(&mut self, fonts: &FontRegistry, font_id: u32, size: Fx) -> Fx {
        let (_, desc, upem) = self.get(fonts, font_id);
        Fx::scale_font_units(desc, size, upem)
    }
}

pub(super) fn rect(r: FxRect) -> Rect {
    Rect {
        x: f64::from(r.x.to_f32()),
        y: f64::from(r.y.to_f32()),
        width: f64::from(r.w.to_f32()),
        height: f64::from(r.h.to_f32()),
    }
}

/// Builds one page's display list and query maps. Infallible: a purely
/// mechanical transform of already-validated layout.
pub fn build_page(page: &LaidPage, ctx: &DisplayContext<'_>) -> (PageDisplayList, PageMaps) {
    let vertical = ctx.styled.writing_mode == WritingMode::VerticalRl;
    let mut metrics = FaceMetrics::default();
    let mut glyph_runs = Vec::new();
    let mut decorations = Vec::new();
    let mut links = Vec::new();
    let mut line_geoms = Vec::with_capacity(page.lines.len());

    for pl in &page.lines {
        line_geoms.push(emit_line(
            pl,
            ctx,
            vertical,
            &mut metrics,
            &mut glyph_runs,
            &mut decorations,
            &mut links,
        ));
    }
    for d in &page.decorations {
        decorations.push(Decoration {
            kind: DecorationKind::Rule,
            rect: rect(d.rect),
            color_role: ColorRole::Secondary,
        });
    }
    let images = page
        .images
        .iter()
        .map(|img| ImagePlacement {
            href: img.href.clone(),
            rect: rect(img.frame),
        })
        .collect();
    let a11y = a11y::blocks(page, ctx, vertical);

    (
        PageDisplayList {
            generation: ctx.generation,
            glyph_runs,
            images,
            decorations,
            links,
            a11y,
        },
        PageMaps {
            char_range: page.char_range.clone(),
            lines: line_geoms,
        },
    )
}

/// Emits one placed line: glyph runs split at color-role changes, link
/// regions with their underlines, and the line's [`LineGeom`].
fn emit_line(
    pl: &PlacedLine,
    ctx: &DisplayContext<'_>,
    vertical: bool,
    metrics: &mut FaceMetrics,
    glyph_runs: &mut Vec<GlyphRun>,
    decorations: &mut Vec<Decoration>,
    links: &mut Vec<LinkRegion>,
) -> LineGeom {
    let line = &pl.line;
    let mut cells: Vec<(u64, Fx, Fx)> = Vec::new();
    // Per link on this line: (link index, inline min, inline max), in
    // first-appearance order — never hash-ordered.
    let mut line_links: Vec<(usize, Fx, Fx)> = Vec::new();

    for pr in &line.runs {
        let is_ruby = pr.run.style.is_ruby;
        // Block-axis anchor: the baseline (horizontal) or column axis
        // (vertical). Ruby annotations sit on the over side, their own
        // descent inset into the ruby band.
        let axis = if is_ruby {
            let desc = metrics.descent(ctx.fonts, pr.run.font_id, pr.run.size);
            if vertical {
                pl.x + line.ascent + desc
            } else {
                pl.y - line.ascent - desc
            }
        } else if vertical {
            pl.x
        } else {
            pl.y
        };
        // Inline pen, absolute page coordinate along the inline axis.
        let inline_origin = if vertical { pl.y } else { pl.x };
        let mut pen = inline_origin + pr.inline_offset;

        let mut run: Option<GlyphRun> = None;
        for g in &pr.run.glyphs {
            let c = u64::from(g.cluster);
            let link = if is_ruby { None } else { ctx.link_at(c) };
            let color = if is_ruby {
                ColorRole::Secondary
            } else if link.is_some() {
                ColorRole::Link
            } else {
                ColorRole::Text
            };
            let (x, y) = match (vertical, pr.run.orientation) {
                (false, _) => (pen + g.offset_x, axis - g.offset_y),
                (true, RunOrientation::Upright) => (axis + g.offset_x, pen - g.offset_y),
                // Rotated 90° clockwise about the emitted position:
                // the (x_offset, −y_offset) page vector maps to
                // (y_offset, x_offset).
                (true, RunOrientation::SidewaysRotated) => (axis + g.offset_y, pen + g.offset_x),
            };
            let start_new = run.as_ref().is_none_or(|r| r.color_role != color);
            if start_new {
                if let Some(done) = run.take() {
                    glyph_runs.push(done);
                }
                run = Some(GlyphRun {
                    font_id: pr.run.font_id,
                    size: f64::from(pr.run.size.to_f32()),
                    color_role: color,
                    glyph_ids: Vec::new(),
                    positions: Vec::new(),
                    orientation: pr.run.orientation,
                });
            }
            if let Some(r) = run.as_mut() {
                r.glyph_ids.push(g.glyph_id);
                r.positions.push(x.to_f32());
                r.positions.push(y.to_f32());
            }
            if !is_ruby {
                // Cells merge same-cluster glyphs (marks) in place.
                match cells.last_mut() {
                    Some((cc, _, extent)) if *cc == c => *extent += g.advance,
                    _ => cells.push((c, pen, g.advance)),
                }
                if let Some(li) = link {
                    let end = pen + g.advance;
                    match line_links.iter_mut().find(|(i, ..)| *i == li) {
                        Some((_, min, max)) => {
                            if pen < *min {
                                *min = pen;
                            }
                            if end > *max {
                                *max = end;
                            }
                        }
                        None => line_links.push((li, pen, end)),
                    }
                }
            }
            pen += g.advance;
        }
        if let Some(done) = run.take() {
            glyph_runs.push(done);
        }
    }

    emit_link_geometry(pl, ctx, vertical, &line_links, decorations, links);

    let band = if vertical {
        FxRect {
            x: pl.x - line.descent,
            y: Fx::ZERO,
            w: line.descent + line.ascent + line.ruby_extent,
            h: ctx.viewport.height,
        }
    } else {
        FxRect {
            x: Fx::ZERO,
            y: pl.y - line.ascent - line.ruby_extent,
            w: ctx.viewport.width,
            h: line.ascent + line.ruby_extent + line.descent,
        }
    };
    LineGeom {
        char_range: line.char_range.clone(),
        band,
        cells,
    }
}

/// One [`LinkRegion`] + one `Link`-colored underline per link stretch
/// on the line.
fn emit_link_geometry(
    pl: &PlacedLine,
    ctx: &DisplayContext<'_>,
    vertical: bool,
    line_links: &[(usize, Fx, Fx)],
    decorations: &mut Vec<Decoration>,
    links: &mut Vec<LinkRegion>,
) {
    let line = &pl.line;
    for &(li, min, max) in line_links {
        let region = if vertical {
            FxRect {
                x: pl.x - line.descent,
                y: min,
                w: line.descent + line.ascent,
                h: max - min,
            }
        } else {
            FxRect {
                x: min,
                y: pl.y - line.ascent,
                w: max - min,
                h: line.ascent + line.descent,
            }
        };
        // Underline: a hairline centered halfway into the descent, on
        // the under side (below in horizontal, left of the column in
        // vertical-rl).
        let underline = if vertical {
            FxRect {
                x: pl.x - Fx(line.descent.0 / 2) - Fx(HAIRLINE.0 / 2),
                y: min,
                w: HAIRLINE,
                h: max - min,
            }
        } else {
            FxRect {
                x: min,
                y: pl.y + Fx(line.descent.0 / 2) - Fx(HAIRLINE.0 / 2),
                w: max - min,
                h: HAIRLINE,
            }
        };
        links.push(LinkRegion {
            rect: rect(region),
            target: ctx.links[li].target.clone(),
        });
        decorations.push(Decoration {
            kind: DecorationKind::Underline,
            rect: rect(underline),
            color_role: ColorRole::Link,
        });
    }
}
