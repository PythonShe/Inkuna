//! Image measurement and sizing. Intrinsic dimensions are
//! header-sniffed (`imagesize`) from bytes the CALLER hands in via
//! [`super::ChapterInput::resources`] — the engine is a pure library
//! and never reads an archive itself; M5's session worker does. Every
//! failure (missing resource, unsniffable bytes, absurd dimensions)
//! degrades to the fixed placeholder box — never an error, never a
//! dropped block — still carrying the href so the shell can show its
//! broken-image treatment.

use std::collections::HashMap;

use crate::fixed::Fx;

use super::pages::FxRect;

/// Memo of sniffed intrinsic dimensions, keyed by resolved href — one
/// per `paginate` call, so each unique href is fetched and sniffed at
/// most once per pagination pass (the heading-keep lookahead would
/// otherwise sniff the same image twice). Never outlives the pass.
pub(super) type DimensionCache = HashMap<String, Option<(u32, u32)>>;

/// One image placed on a page.
pub struct PlacedImage {
    /// Normalized package-root-relative path, resolved via
    /// inkuna-content against the chapter resource's own path.
    pub href: String,
    /// In page coordinates.
    pub frame: FxRect,
}

/// Sniffed width or height beyond this is treated as hostile input and
/// degrades to the placeholder.
const MAX_IMAGE_DIMENSION_PX: u32 = 16_384;

/// The placeholder box: 160 × 160 pt, centered on the inline axis.
const PLACEHOLDER: Fx = Fx(160 * 64);

/// One image block, measured: its resolved href and its intrinsic
/// pixel size — `None` degrades to the placeholder box.
pub(super) struct MeasuredImage {
    pub href: String,
    pub intrinsic: Option<(u32, u32)>,
}

/// Resolves and header-sniffs one image, memoized per pagination pass
/// via `cache`. Deterministic: the outcome depends only on the src, the
/// resource path, and the bytes returned.
pub(super) fn measure_image(
    src: &str,
    resource_path: &str,
    resources: &dyn Fn(&str) -> Option<Vec<u8>>,
    cache: &mut DimensionCache,
) -> MeasuredImage {
    let resolved = inkuna_content::resolve_relative(resource_path, src);
    let (path, _) = inkuna_content::split_fragment(&resolved);
    let href = path.to_string();
    if let Some(&intrinsic) = cache.get(&href) {
        return MeasuredImage { href, intrinsic };
    }
    let intrinsic = sniff(&href, resources);
    cache.insert(href.clone(), intrinsic);
    MeasuredImage { href, intrinsic }
}

/// One resource fetch + header sniff; every failure degrades to `None`
/// (the placeholder box).
fn sniff(href: &str, resources: &dyn Fn(&str) -> Option<Vec<u8>>) -> Option<(u32, u32)> {
    match resources(href) {
        None => {
            log::debug!("image resource missing: {href}");
            None
        }
        Some(bytes) => match imagesize::blob_size(&bytes) {
            Ok(size) => {
                let (w, h) = (size.width as u64, size.height as u64);
                if w == 0
                    || h == 0
                    || w > u64::from(MAX_IMAGE_DIMENSION_PX)
                    || h > u64::from(MAX_IMAGE_DIMENSION_PX)
                {
                    log::debug!("image dimensions out of budget: {href} ({w}x{h})");
                    None
                } else {
                    Some((w as u32, h as u32))
                }
            }
            Err(e) => {
                log::debug!("image unsniffable: {href} ({e})");
                None
            }
        },
    }
}

/// The drawn size: intrinsic px as layout points at 1×, fitted within
/// the content box preserving aspect ratio, never upscaled. `None`
/// intrinsic yields the fixed placeholder box. Exact rational
/// arithmetic — no floats.
pub(super) fn fit(intrinsic: Option<(u32, u32)>, box_w: Fx, box_h: Fx) -> (Fx, Fx) {
    let Some((iw, ih)) = intrinsic else {
        return (PLACEHOLDER, PLACEHOLDER);
    };
    // <= 16_384 px, so × 64 stays well inside i32.
    let w = Fx((iw as i32).saturating_mul(64));
    let h = Fx((ih as i32).saturating_mul(64));
    if w <= box_w && h <= box_h {
        return (w, h);
    }
    // Which axis binds: compare aspect ratios by cross-multiplication.
    let width_bound = i64::from(w.0) * i64::from(box_h.0) >= i64::from(h.0) * i64::from(box_w.0);
    if width_bound {
        (box_w, box_w.mul_ratio(ih as i32, iw as i32))
    } else {
        (box_h.mul_ratio(iw as i32, ih as i32), box_h)
    }
}
