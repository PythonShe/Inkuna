//! The computed-style values the engine honors. Deliberately tiny: the
//! reader's typography is settings-owned, so publisher CSS only steers
//! structure (visibility, direction, writing mode) and emphasis.

use crate::dom::Document;

/// Per-resource writing mode, read only from `html`/`body` rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WritingMode {
    #[default]
    HorizontalTb,
    VerticalRl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
}

/// Numeric weights ≥ 600 map to `Bold` at parse time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontWeight {
    #[default]
    Normal,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    Center,
    End,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RubyPosition {
    #[default]
    Over,
    Under,
}

/// One node's computed style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComputedStyle {
    pub display_none: bool,
    pub direction: Direction,
    pub font_style: FontStyle,
    pub font_weight: FontWeight,
    pub text_align: TextAlign,
    pub ruby_position: RubyPosition,
}

impl Default for ComputedStyle {
    /// The UA initial values; `Justify` is the default for body text —
    /// reader settings, not publisher CSS, own the visual typography.
    fn default() -> Self {
        Self {
            display_none: false,
            direction: Direction::Ltr,
            font_style: FontStyle::Normal,
            font_weight: FontWeight::Normal,
            text_align: TextAlign::Justify,
            ruby_position: RubyPosition::Over,
        }
    }
}

/// A document plus its resolved styles, parallel to `doc.nodes`.
#[derive(Debug)]
pub struct StyledDocument<'d> {
    pub doc: &'d Document,
    pub styles: Vec<ComputedStyle>,
    pub writing_mode: WritingMode,
}
