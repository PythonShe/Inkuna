//! Ruby: a base text plus an annotation shaped at the settings' exact
//! ruby ratio. The annotation is display-only — its glyphs carry the
//! BASE run's first char offset as their cluster, so selection and
//! hit-testing always land on the base ("selectable as its base").

use crate::fixed::Fx;
use crate::style::RubyPosition;

use super::shape::{shape_text, ShapeContext, ShapedRun};

/// One ruby pair, ready for M4's line layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RubyRun {
    pub base: Vec<ShapedRun>,
    pub annotation: Vec<ShapedRun>,
    pub position: RubyPosition,
}

/// Shape a ruby pair. The annotation shapes at
/// `ctx.size.mul_ratio(num, den)` (`Typography::ruby_scale`'s exact
/// ratio), marked `is_ruby`, with letter/word spacing zeroed — reader
/// tracking applies to body text, not annotations. `position` starts
/// at its default; the caller sets it from the node's computed
/// `ruby_position`. An empty `rt_text` yields an empty annotation
/// (renders as plain base).
pub fn shape_ruby(
    base_text: &str,
    rt_text: &str,
    ctx: &ShapeContext,
    ruby_scale: (u32, u32),
) -> RubyRun {
    let base = shape_text(base_text, ctx);
    if rt_text.is_empty() {
        return RubyRun {
            base,
            annotation: Vec::new(),
            position: RubyPosition::default(),
        };
    }
    let (num, den) = ruby_scale;
    let annotation_ctx = ShapeContext {
        size: ctx.size.mul_ratio(num as i32, den as i32),
        letter_spacing: Fx::ZERO,
        word_spacing: Fx::ZERO,
        ..*ctx
    };
    let mut annotation = shape_text(rt_text, &annotation_ctx);
    // The base's first char offset — shape_text offsets start at 0.
    let base_start = 0u32;
    for run in &mut annotation {
        run.style.is_ruby = true;
        for glyph in &mut run.glyphs {
            glyph.cluster = base_start;
        }
    }
    RubyRun {
        base,
        annotation,
        position: RubyPosition::default(),
    }
}
