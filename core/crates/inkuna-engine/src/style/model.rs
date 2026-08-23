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

/// Inline-axis base direction, inherited down the tree. Seeded from the
/// element's `dir` attribute and overridden by a publisher `direction`
/// declaration. Orthogonal to [`WritingMode`]: a vertical-rl chapter
/// still resolves each line's inline direction with this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

/// Slant, resolved to the two states the bundled roster actually has
/// faces for. `oblique` folds into `Italic` at parse time; `<em>` sets
/// it as a UA default.
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

/// Inline alignment of a paragraph's lines. `Start`/`End` are
/// direction-relative, not physical: the parser folds `left` into
/// `Start` and `right` into `End`, and line breaking resolves them
/// against [`Direction`]. There is no [`Default`] impl on purpose —
/// `ComputedStyle::default()` picks `Justify` for body text, which is a
/// reader decision rather than a UA initial value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    Center,
    End,
    Justify,
}

/// Which side of the base text a ruby annotation sits on. `Over` and
/// `Under` are relative to the line, not the screen: in vertical-rl
/// writing `Over` is the right of the column and `Under` its left.
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
