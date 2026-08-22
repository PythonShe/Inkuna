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
use crate::paginate::{FxRect, LaidPage, PlacedLine};
use crate::shape::RunOrientation;
use crate::style::{RubyPosition, WritingMode};

use super::a11y;
use super::context::{DisplayContext, FaceMetrics};
use super::maps::{LineGeom, PageMaps};

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;

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
        // Exhaustive on the paginate-side kind: adding a
        // `PlacedDecorationKind` variant must fail compilation here,
        // never silently mislabel as a rule.
        let (kind, color_role) = match d.kind {
            crate::paginate::DecorationKind::Rule => {
                (DecorationKind::Rule, ColorRole::Secondary)
            }
        };
        decorations.push(Decoration {
            kind,
            rect: rect(d.rect),
            color_role,
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
        // (vertical). Ruby annotations occupy their requested side, with
        // their metric inset into that side's band.
        let axis = if is_ruby {
            let desc = metrics.descent(ctx.fonts, pr.run.font_id, pr.run.size);
            let under = pr.ruby_position == Some(RubyPosition::Under);
            if vertical {
                if under {
                    pl.x - line.descent - metrics.ascent(ctx.fonts, pr.run.font_id, pr.run.size)
                } else {
                    pl.x + line.ascent + desc
                }
            } else if under {
                pl.y + line.descent + metrics.ascent(ctx.fonts, pr.run.font_id, pr.run.size)
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
            x: pl.x - line.descent - line.ruby_under_extent,
            y: Fx::ZERO,
            w: line.ruby_under_extent + line.descent + line.ascent + line.ruby_over_extent,
            h: ctx.viewport.height,
        }
    } else {
        FxRect {
            x: Fx::ZERO,
            y: pl.y - line.ascent - line.ruby_over_extent,
            w: ctx.viewport.width,
            h: line.ruby_over_extent + line.ascent + line.descent + line.ruby_under_extent,
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
