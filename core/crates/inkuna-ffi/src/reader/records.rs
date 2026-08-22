//! The reader records mirrored 1:1 from `inkuna-core`'s engine
//! re-exports, with `From` conversions beside each. Geometry contract:
//! positions and rects are page coordinates in layout points at 1× (y
//! grows downward); glyph positions are horizontal-baseline origins,
//! and `SidewaysRotated` runs draw rotated 90° clockwise about each
//! glyph's emitted position.

/// A content coordinate: THE reading position. `char_offset` counts
/// Unicode scalars into the resource's canonical projection, so the
/// same coordinate means the same character on every platform and
/// under every layout setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct Coordinate {
    /// Index into the spine, in reading order.
    pub spine_idx: u32,
    /// Unicode scalar count into the resource's canonical projection.
    pub char_offset: u64,
}

impl From<inkuna_core::Coordinate> for Coordinate {
    fn from(c: inkuna_core::Coordinate) -> Self {
        Coordinate {
            spine_idx: c.spine_idx,
            char_offset: c.char_offset,
        }
    }
}

impl From<Coordinate> for inkuna_core::Coordinate {
    fn from(c: Coordinate) -> Self {
        inkuna_core::Coordinate {
            spine_idx: c.spine_idx,
            char_offset: c.char_offset,
        }
    }
}

/// The page frame in layout points at 1×.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
}

impl From<Viewport> for inkuna_core::Viewport {
    fn from(v: Viewport) -> Self {
        inkuna_core::Viewport {
            width: v.width,
            height: v.height,
        }
    }
}

/// The six Customize settings as the shells persist them; out-of-range
/// values clamp engine-side and unknown fonts default, never error.
/// `reading_margins` is inline-axis page padding in layout points
/// (numerically identical to the stored CSS px at 1×).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ReaderLayoutSettings {
    /// Roster font id, opaque (`"noto-serif"`, `"publisher"`, …).
    pub reading_font: String,
    pub reading_bold: bool,
    /// Text size step, 0..=4.
    pub text_size_step: u8,
    /// Line height over body size, clamped to 1.30..=2.10.
    pub line_spacing: f64,
    /// Extra letter spacing in em, clamped to 0.0..=0.06.
    pub letter_spacing: f64,
    /// Extra word spacing in em, clamped to 0.0..=0.30.
    pub word_spacing: f64,
    /// Inline-axis page margins in layout points, clamped to 16..=48.
    pub reading_margins: u32,
}

impl From<ReaderLayoutSettings> for inkuna_core::LayoutSettings {
    fn from(s: ReaderLayoutSettings) -> Self {
        inkuna_core::LayoutSettings {
            reading_font: s.reading_font,
            reading_bold: s.reading_bold,
            text_size_step: s.text_size_step,
            line_spacing: s.line_spacing,
            letter_spacing: s.letter_spacing,
            word_spacing: s.word_spacing,
            reading_margins: s.reading_margins,
        }
    }
}

/// A canonical char range, `end` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct CharRange {
    pub start: u64,
    pub end: u64,
}

impl From<inkuna_core::CharRange> for CharRange {
    fn from(r: inkuna_core::CharRange) -> Self {
        CharRange {
            start: r.start,
            end: r.end,
        }
    }
}

impl From<CharRange> for inkuna_core::CharRange {
    fn from(r: CharRange) -> Self {
        inkuna_core::CharRange {
            start: r.start,
            end: r.end,
        }
    }
}

/// A rectangle in page coordinates, layout points at 1×.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl From<inkuna_core::Rect> for Rect {
    fn from(r: inkuna_core::Rect) -> Self {
        Rect {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum WritingMode {
    HorizontalTb,
    VerticalRl,
}

impl From<inkuna_core::WritingMode> for WritingMode {
    fn from(m: inkuna_core::WritingMode) -> Self {
        match m {
            inkuna_core::WritingMode::HorizontalTb => WritingMode::HorizontalTb,
            inkuna_core::WritingMode::VerticalRl => WritingMode::VerticalRl,
        }
    }
}

/// One selection rect with the writing mode the shell needs to draw its
/// handles correctly.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct SelectionRect {
    pub rect: Rect,
    pub writing_mode: WritingMode,
}

impl From<inkuna_core::SelectionRect> for SelectionRect {
    fn from(r: inkuna_core::SelectionRect) -> Self {
        SelectionRect {
            rect: r.rect.into(),
            writing_mode: r.writing_mode.into(),
        }
    }
}

/// One chapter's laid-out geometry. `truncated` means the resource hit
/// a parse/layout budget and rendered only its laid-out prefix (shells
/// show a notice).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ChapterGeometry {
    pub generation: u64,
    pub page_count: u32,
    pub char_range: CharRange,
    pub writing_mode: WritingMode,
    /// The package-level `page-progression-direction="rtl"` flag.
    pub rtl_progression: bool,
    pub truncated: bool,
}

impl From<inkuna_core::ChapterGeometry> for ChapterGeometry {
    fn from(g: inkuna_core::ChapterGeometry) -> Self {
        ChapterGeometry {
            generation: g.generation,
            page_count: g.page_count,
            char_range: g.char_range.into(),
            writing_mode: g.writing_mode.into(),
            rtl_progression: g.rtl_progression,
            truncated: g.truncated,
        }
    }
}

/// A located page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct PageLocation {
    pub generation: u64,
    pub spine_idx: u32,
    pub page_idx: u32,
}

impl From<inkuna_core::PageLocation> for PageLocation {
    fn from(l: inkuna_core::PageLocation) -> Self {
        PageLocation {
            generation: l.generation,
            spine_idx: l.spine_idx,
            page_idx: l.page_idx,
        }
    }
}

/// A hit-test result: the resolved coordinate plus the link target when
/// the point lands inside a link region.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HitResult {
    pub coordinate: Coordinate,
    pub link_target: Option<String>,
}

impl From<inkuna_core::HitResult> for HitResult {
    fn from(h: inkuna_core::HitResult) -> Self {
        HitResult {
            coordinate: h.coordinate.into(),
            link_target: h.link_target,
        }
    }
}

/// The shells' theme-mapped color slots. Core assigns roles; shells
/// resolve them to actual colors per theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ColorRole {
    Text,
    Secondary,
    Link,
}

impl From<inkuna_core::ColorRole> for ColorRole {
    fn from(c: inkuna_core::ColorRole) -> Self {
        match c {
            inkuna_core::ColorRole::Text => ColorRole::Text,
            inkuna_core::ColorRole::Secondary => ColorRole::Secondary,
            inkuna_core::ColorRole::Link => ColorRole::Link,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RunOrientation {
    Upright,
    SidewaysRotated,
}

impl From<inkuna_core::RunOrientation> for RunOrientation {
    fn from(o: inkuna_core::RunOrientation) -> Self {
        match o {
            inkuna_core::RunOrientation::Upright => RunOrientation::Upright,
            inkuna_core::RunOrientation::SidewaysRotated => RunOrientation::SidewaysRotated,
        }
    }
}

/// One drawable glyph sequence: a single face at a single size and
/// color role. `positions` is x,y interleaved, len = 2 × glyphs.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct GlyphRun {
    pub font_id: u32,
    pub size: f64,
    pub color_role: ColorRole,
    pub glyph_ids: Vec<u16>,
    pub positions: Vec<f32>,
    pub orientation: RunOrientation,
}

impl From<inkuna_core::GlyphRun> for GlyphRun {
    fn from(r: inkuna_core::GlyphRun) -> Self {
        GlyphRun {
            font_id: r.font_id,
            size: r.size,
            color_role: r.color_role.into(),
            glyph_ids: r.glyph_ids,
            positions: r.positions,
            orientation: r.orientation.into(),
        }
    }
}

/// One image at its page frame; `href` is normalized
/// package-root-relative.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ImagePlacement {
    pub href: String,
    pub rect: Rect,
}

impl From<inkuna_core::ImagePlacement> for ImagePlacement {
    fn from(i: inkuna_core::ImagePlacement) -> Self {
        ImagePlacement {
            href: i.href,
            rect: i.rect.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DecorationKind {
    Rule,
    Underline,
}

impl From<inkuna_core::DecorationKind> for DecorationKind {
    fn from(k: inkuna_core::DecorationKind) -> Self {
        match k {
            inkuna_core::DecorationKind::Rule => DecorationKind::Rule,
            inkuna_core::DecorationKind::Underline => DecorationKind::Underline,
        }
    }
}

/// A filled rect the shell draws in the role's color — roles are
/// core-assigned, never shell-inferred.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Decoration {
    pub kind: DecorationKind,
    pub rect: Rect,
    pub color_role: ColorRole,
}

impl From<inkuna_core::Decoration> for Decoration {
    fn from(d: inkuna_core::Decoration) -> Self {
        Decoration {
            kind: d.kind.into(),
            rect: d.rect.into(),
            color_role: d.color_role.into(),
        }
    }
}

/// A tappable link area. Internal targets are normalized
/// package-root-relative hrefs (fragment kept); external
/// `http(s):`/`mailto:` targets are kept verbatim.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LinkRegion {
    pub rect: Rect,
    pub target: String,
}

impl From<inkuna_core::LinkRegion> for LinkRegion {
    fn from(l: inkuna_core::LinkRegion) -> Self {
        LinkRegion {
            rect: l.rect.into(),
            target: l.target,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum A11yRole {
    Body,
    Heading,
    Link,
}

impl From<inkuna_core::A11yRole> for A11yRole {
    fn from(r: inkuna_core::A11yRole) -> Self {
        match r {
            inkuna_core::A11yRole::Body => A11yRole::Body,
            inkuna_core::A11yRole::Heading => A11yRole::Heading,
            inkuna_core::A11yRole::Link => A11yRole::Link,
        }
    }
}

/// One block-level element's accessible text on this page, in logical
/// reading order. Ruby readings are appended parenthetically
/// ("漢字（かんじ）").
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct A11yBlock {
    pub text: String,
    pub rect: Rect,
    /// Nearest ancestor `lang`/`xml:lang`, `None` when unset.
    pub lang: Option<String>,
    pub is_link: bool,
    pub role: A11yRole,
}

impl From<inkuna_core::A11yBlock> for A11yBlock {
    fn from(b: inkuna_core::A11yBlock) -> Self {
        A11yBlock {
            text: b.text,
            rect: b.rect.into(),
            lang: b.lang,
            is_link: b.is_link,
            role: b.role.into(),
        }
    }
}

/// Everything the shell draws for one page. All vectors are in
/// deterministic construction order (visual order within lines, lines
/// in block order).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PageDisplayList {
    pub generation: u64,
    pub glyph_runs: Vec<GlyphRun>,
    pub images: Vec<ImagePlacement>,
    pub decorations: Vec<Decoration>,
    pub links: Vec<LinkRegion>,
    pub a11y: Vec<A11yBlock>,
}

impl From<inkuna_core::PageDisplayList> for PageDisplayList {
    fn from(p: inkuna_core::PageDisplayList) -> Self {
        PageDisplayList {
            generation: p.generation,
            glyph_runs: p.glyph_runs.into_iter().map(Into::into).collect(),
            images: p.images.into_iter().map(Into::into).collect(),
            decorations: p.decorations.into_iter().map(Into::into).collect(),
            links: p.links.into_iter().map(Into::into).collect(),
            a11y: p.a11y.into_iter().map(Into::into).collect(),
        }
    }
}

/// One variation-axis coordinate a face is used at.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FontAxis {
    pub tag: String,
    pub value: f64,
}

impl From<inkuna_core::FontAxis> for FontAxis {
    fn from(a: inkuna_core::FontAxis) -> Self {
        FontAxis {
            tag: a.tag,
            value: a.value,
        }
    }
}

/// One registry face; shells rebuild platform fonts from `file_path` +
/// `collection_index`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FontEntry {
    pub id: u32,
    /// Absolute path under the registry's font dir.
    pub file_path: String,
    pub collection_index: u32,
    pub axes: Vec<FontAxis>,
}

impl From<inkuna_core::FontEntry> for FontEntry {
    fn from(e: inkuna_core::FontEntry) -> Self {
        FontEntry {
            id: e.id,
            file_path: e.file_path,
            collection_index: e.collection_index,
            axes: e.axes.into_iter().map(Into::into).collect(),
        }
    }
}
