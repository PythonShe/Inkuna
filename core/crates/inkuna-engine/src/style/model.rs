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

/// A CSS numeric font weight, clamped to 1..=1000 (the CSS
/// `font-weight` range). `normal` is 400, `bold` 700; the registry maps
/// any value onto its nearest available face or `wght` instance, and
/// the static CJK/Hebrew faces threshold at [`FontWeight::is_bold`]
/// (≥ 600 → Bold).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontWeight(u16);

impl FontWeight {
    /// The CSS initial value (`normal`).
    pub const NORMAL: FontWeight = FontWeight(400);
    /// CSS `bold`.
    pub const BOLD: FontWeight = FontWeight(700);

    /// Clamps into the CSS 1..=1000 range; 0 clamps up to 1.
    pub const fn new(value: u16) -> Self {
        Self(if value < 1 {
            1
        } else if value > 1000 {
            1000
        } else {
            value
        })
    }

    pub const fn value(self) -> u16 {
        self.0
    }

    /// The static-face bold threshold: the pre-engine WebView renderer
    /// treated ≥ 600 as bold, and the static CJK/Hebrew R/B pairs keep
    /// that semantic.
    pub const fn is_bold(self) -> bool {
        self.0 >= 600
    }

    /// CSS `bolder`, resolved against `self` as the inherited weight
    /// (css-fonts-4 §font-weight relative-weight table).
    pub const fn bolder(self) -> Self {
        match self.0 {
            0..350 => Self(400),
            350..550 => Self(700),
            550..900 => Self(900),
            _ => self,
        }
    }

    /// CSS `lighter`, resolved against `self` as the inherited weight
    /// (css-fonts-4 §font-weight relative-weight table).
    pub const fn lighter(self) -> Self {
        match self.0 {
            0..100 => self,
            100..550 => Self(100),
            550..750 => Self(400),
            _ => Self(700),
        }
    }
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
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
            font_weight: FontWeight::NORMAL,
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
