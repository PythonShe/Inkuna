//! Canonical serialization + blake3 digest of one page display list —
//! the cross-platform parity currency. The FFI's `page_digest` (M6) and
//! the golden tests both call [`page_digest`]; the device parity harness
//! compares these hex strings across platforms.
//!
//! The byte layout is specified EXACTLY, and any change to it is a
//! parity-breaking event:
//!
//! - all integers little-endian, fixed width;
//! - each vector as `u32 len` followed by its elements in order;
//! - strings as `u64 len` + UTF-8 bytes;
//! - `Option<String>` as a `u8` presence flag (0/1) + the string when 1;
//! - `f32`/`f64` as IEEE-754 `to_bits()` LE (values originate in
//!   `Fx::to_f32`, so bit patterns are identical cross-platform by
//!   construction);
//! - enums as `u8` discriminants in declaration order;
//! - fields within each record in declaration order.
//!
//! Vectors are deterministically ordered by construction — nothing in
//! `display/` iterates a hash-ordered collection.

use crate::shape::RunOrientation;

use super::list::{
    A11yBlock, A11yRole, ColorRole, Decoration, DecorationKind, GlyphRun, ImagePlacement,
    LinkRegion, PageDisplayList, Rect,
};

/// Lowercase blake3 hex (64 chars) of `bytes`.
pub fn digest_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Lowercase blake3 hex (64 chars) of the list's canonical bytes. The
/// digest is pure page-content identity: session counters such as
/// [`PageDisplayList::generation`] are intentionally excluded.
pub fn page_digest(list: &PageDisplayList) -> String {
    digest_hex(&canonical_bytes(list))
}

/// The canonical byte serialization the module doc specifies.
fn canonical_bytes(list: &PageDisplayList) -> Vec<u8> {
    let mut out = Vec::new();
    vec_header(&mut out, list.glyph_runs.len());
    for run in &list.glyph_runs {
        glyph_run(&mut out, run);
    }
    vec_header(&mut out, list.images.len());
    for img in &list.images {
        image(&mut out, img);
    }
    vec_header(&mut out, list.decorations.len());
    for d in &list.decorations {
        decoration(&mut out, d);
    }
    vec_header(&mut out, list.links.len());
    for l in &list.links {
        link(&mut out, l);
    }
    vec_header(&mut out, list.a11y.len());
    for b in &list.a11y {
        a11y(&mut out, b);
    }
    out
}

/// Vector lengths are `u32`; every display vector is far below the cap
/// by the layout budgets, and saturating keeps this panic-free anyway.
fn vec_header(out: &mut Vec<u8>, len: usize) {
    out.extend_from_slice(&(len.min(u32::MAX as usize) as u32).to_le_bytes());
}

fn string(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u64).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn rect(out: &mut Vec<u8>, r: &Rect) {
    out.extend_from_slice(&r.x.to_bits().to_le_bytes());
    out.extend_from_slice(&r.y.to_bits().to_le_bytes());
    out.extend_from_slice(&r.width.to_bits().to_le_bytes());
    out.extend_from_slice(&r.height.to_bits().to_le_bytes());
}

fn color_role(out: &mut Vec<u8>, c: ColorRole) {
    out.push(match c {
        ColorRole::Text => 0,
        ColorRole::Secondary => 1,
        ColorRole::Link => 2,
    });
}

fn glyph_run(out: &mut Vec<u8>, run: &GlyphRun) {
    out.extend_from_slice(&run.font_id.to_le_bytes());
    out.extend_from_slice(&run.size.to_bits().to_le_bytes());
    color_role(out, run.color_role);
    vec_header(out, run.glyph_ids.len());
    for id in &run.glyph_ids {
        out.extend_from_slice(&id.to_le_bytes());
    }
    vec_header(out, run.positions.len());
    for p in &run.positions {
        out.extend_from_slice(&p.to_bits().to_le_bytes());
    }
    out.push(match run.orientation {
        RunOrientation::Upright => 0,
        RunOrientation::SidewaysRotated => 1,
    });
}

fn image(out: &mut Vec<u8>, img: &ImagePlacement) {
    string(out, &img.href);
    rect(out, &img.rect);
}

fn decoration(out: &mut Vec<u8>, d: &Decoration) {
    out.push(match d.kind {
        DecorationKind::Rule => 0,
        DecorationKind::Underline => 1,
    });
    rect(out, &d.rect);
    color_role(out, d.color_role);
}

fn link(out: &mut Vec<u8>, l: &LinkRegion) {
    rect(out, &l.rect);
    string(out, &l.target);
}

fn a11y(out: &mut Vec<u8>, b: &A11yBlock) {
    string(out, &b.text);
    rect(out, &b.rect);
    match &b.lang {
        None => out.push(0),
        Some(lang) => {
            out.push(1);
            string(out, lang);
        }
    }
    out.push(u8::from(b.is_link));
    out.push(match b.role {
        A11yRole::Body => 0,
        A11yRole::Heading => 1,
        A11yRole::Link => 2,
    });
}
