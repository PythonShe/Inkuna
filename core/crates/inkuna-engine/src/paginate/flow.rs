//! The page-flow state machine: places lines and decorations into the
//! page's content box, breaks pages at line boundaries with the
//! widow/orphan rule, and emits each page the moment it is full.
//!
//! Axis handling: in horizontal writing the block axis is `y` (top to
//! bottom); in vertical-rl the lines are columns, the inline axis is
//! vertical, and the block axis advances right-to-left — geometry is
//! emitted in normal page coordinates with `x` measured from the right
//! edge. RTL page progression never touches geometry here; it is a
//! flag surfaced by the session layer only.

use crate::fixed::Fx;
use crate::layout::Line;
use crate::style::WritingMode;

use super::images::{self, MeasuredImage, PlacedImage};
use super::pages::{
    ChapterInput, DecorationKind, FxRect, LaidPage, Metrics, PlacedDecoration, PlacedLine,
    MAX_PAGES_PER_CHAPTER,
};

/// A hairline: 1 pt, for `hr` rules.
const HAIRLINE: Fx = Fx(64);

pub(super) struct Pager {
    vertical: bool,
    content_x: Fx,
    content_y: Fx,
    content_w: Fx,
    content_h: Fx,
    block_total: Fx,
    index: u32,
    used: Fx,
    lines: Vec<PlacedLine>,
    images: Vec<PlacedImage>,
    decorations: Vec<PlacedDecoration>,
    page_start: u64,
    cursor: u64,
    pages: u32,
    capped: bool,
}

impl Pager {
    pub fn new(input: &ChapterInput<'_>, m: &Metrics) -> Pager {
        let vertical = input.styled.writing_mode == WritingMode::VerticalRl;
        // Guard degenerate viewports: the content box never collapses
        // below one fixed unit of extent, so layout always terminates.
        let content_w = (input.viewport.width - m.margin - m.margin).max(Fx(64));
        let content_h = (input.viewport.height - m.margin - m.margin).max(Fx(64));
        Pager {
            vertical,
            content_x: m.margin,
            content_y: m.margin,
            content_w,
            content_h,
            block_total: if vertical { content_w } else { content_h },
            index: 0,
            used: Fx::ZERO,
            lines: Vec::new(),
            images: Vec::new(),
            decorations: Vec::new(),
            page_start: 0,
            cursor: 0,
            pages: 0,
            capped: false,
        }
    }

    /// The inline-axis extent lines are broken against.
    pub fn inline_total(&self) -> Fx {
        if self.vertical {
            self.content_h
        } else {
            self.content_w
        }
    }

    /// The page cap tripped: no further layout.
    pub fn stopped(&self) -> bool {
        self.capped
    }

    fn has_content(&self) -> bool {
        !self.lines.is_empty() || !self.images.is_empty() || !self.decorations.is_empty()
    }

    /// Closes the current page and emits it — the progressive path.
    fn flush(&mut self, emit: &mut dyn FnMut(LaidPage)) {
        if self.capped {
            return;
        }
        emit(LaidPage {
            index: self.index,
            char_range: self.page_start..self.cursor,
            lines: std::mem::take(&mut self.lines),
            images: std::mem::take(&mut self.images),
            decorations: std::mem::take(&mut self.decorations),
        });
        self.pages += 1;
        self.index += 1;
        self.used = Fx::ZERO;
        self.page_start = self.cursor;
        if self.pages >= MAX_PAGES_PER_CHAPTER {
            self.capped = true;
        }
    }

    /// Heading keep: breaks the page now if `needed` block extent will
    /// not fit after what this page already holds.
    pub fn require(&mut self, needed: Fx, emit: &mut dyn FnMut(LaidPage)) {
        if self.has_content() && self.used + needed > self.block_total {
            self.flush(emit);
        }
    }

    /// Places a paragraph's lines, splitting across pages under the
    /// widow/orphan rule: a split leaves at least 2 lines on BOTH
    /// sides, pushed earlier when it would not — so 2- and 3-line
    /// paragraphs never split. A line taller than an empty page is
    /// force-placed (the page box clips) so layout always terminates.
    pub fn place_paragraph(
        &mut self,
        lines: Vec<Line>,
        adv: Fx,
        inset: Fx,
        spacing: Fx,
        indent: Fx,
        emit: &mut dyn FnMut(LaidPage),
    ) {
        let mut lines: std::collections::VecDeque<Line> = lines.into();
        if lines.is_empty() || self.capped {
            return;
        }
        if self.has_content() {
            let first = adv + lines[0].ruby_over_extent + lines[0].ruby_under_extent;
            if self.used + spacing + first > self.block_total {
                self.flush(emit);
            } else {
                self.used += spacing;
            }
        }
        let mut first_line = true;
        while !lines.is_empty() {
            if self.capped {
                return;
            }
            let remaining = lines.len();
            let mut cap = 0usize;
            let mut u = self.used;
            for line in &lines {
                let a = adv + line.ruby_over_extent + line.ruby_under_extent;
                if u + a > self.block_total {
                    break;
                }
                u += a;
                cap += 1;
            }
            let mut k = if cap >= remaining {
                remaining
            } else {
                let mut k = cap;
                if remaining - k < 2 {
                    k = remaining.saturating_sub(2);
                }
                if k < 2 {
                    k = 0;
                }
                k
            };
            if k == 0 {
                if self.has_content() {
                    self.flush(emit);
                    continue;
                }
                // A fresh page that still cannot honor the rule: force
                // progress rather than loop.
                k = cap.max(1);
            }
            for _ in 0..k {
                let Some(line) = lines.pop_front() else {
                    break;
                };
                let line_inset = if first_line { inset + indent } else { inset };
                first_line = false;
                self.place_line(line, adv, line_inset);
            }
            if !lines.is_empty() {
                self.flush(emit);
            }
        }
    }

    /// Stacks one line: the baseline advances by the block advance plus
    /// ruby extents on their requested sides (above/below in horizontal,
    /// right/left of the column in vertical-rl).
    fn place_line(&mut self, line: Line, adv: Fx, inset: Fx) {
        let extent = adv + line.ruby_over_extent + line.ruby_under_extent;
        let (x, y) = if self.vertical {
            let right = self.content_x + self.content_w;
            (
                right - self.used - line.ruby_over_extent - line.ascent,
                self.content_y + inset,
            )
        } else {
            (
                self.content_x + inset,
                self.content_y + self.used + line.ruby_over_extent + line.ascent,
            )
        };
        self.cursor = self.cursor.max(line.char_range.end);
        self.lines.push(PlacedLine { line, x, y });
        self.used += extent;
    }

    /// An `hr`: one line-advance of block extent with a centered
    /// hairline rule decoration.
    pub fn place_rule(&mut self, m: &Metrics, emit: &mut dyn FnMut(LaidPage)) {
        if self.capped {
            return;
        }
        let adv = m.line_advance;
        if self.has_content() {
            if self.used + m.para_spacing + adv > self.block_total {
                self.flush(emit);
                if self.capped {
                    return;
                }
            } else {
                self.used += m.para_spacing;
            }
        }
        let rect = if self.vertical {
            let right = self.content_x + self.content_w;
            let center = right - self.used - Fx(adv.0 / 2);
            FxRect {
                x: center - Fx(HAIRLINE.0 / 2),
                y: self.content_y,
                w: HAIRLINE,
                h: self.content_h,
            }
        } else {
            FxRect {
                x: self.content_x,
                y: self.content_y + self.used + Fx((adv - HAIRLINE).0 / 2),
                w: self.content_w,
                h: HAIRLINE,
            }
        };
        self.decorations.push(PlacedDecoration {
            rect,
            kind: DecorationKind::Rule,
        });
        self.used += adv;
    }

    /// The block-axis extent a measured image will occupy once fitted
    /// to this content box — the heading-keep lookahead's measure of an
    /// image follower's minimum placeable unit.
    pub fn image_block_extent(&self, intrinsic: Option<(u32, u32)>) -> Fx {
        let (w, h) = images::fit(intrinsic, self.content_w, self.content_h);
        if self.vertical {
            w
        } else {
            h
        }
    }

    /// Places one measured image in the block flow between lines. It
    /// contributes no chars — its position in `char_range` is the
    /// boundary offset between the surrounding text. An image taller
    /// (block-axis) than the remaining page space moves to the next
    /// page; taller than a whole page was already scaled to fit by
    /// [`images::fit`]. In vertical-rl the content box axes swap with
    /// the flow; the frame is still emitted in page coordinates.
    pub fn place_image(
        &mut self,
        img: MeasuredImage,
        m: &Metrics,
        emit: &mut dyn FnMut(LaidPage),
    ) {
        if self.capped {
            return;
        }
        let (w, h) = images::fit(img.intrinsic, self.content_w, self.content_h);
        let block_extent = if self.vertical { w } else { h };
        if self.has_content() {
            if self.used + m.para_spacing + block_extent > self.block_total {
                self.flush(emit);
                if self.capped {
                    return;
                }
            } else {
                self.used += m.para_spacing;
            }
        }
        let frame = if self.vertical {
            let right = self.content_x + self.content_w;
            FxRect {
                x: right - self.used - w,
                // Centered on the (vertical) inline axis.
                y: self.content_y + Fx((self.content_h - h).0 / 2),
                w,
                h,
            }
        } else {
            FxRect {
                // Centered on the inline axis.
                x: self.content_x + Fx((self.content_w - w).0 / 2),
                y: self.content_y + self.used,
                w,
                h,
            }
        };
        self.images.push(PlacedImage {
            href: img.href,
            frame,
        });
        self.used += block_extent;
    }

    /// Flushes the final page (an empty chapter still yields one page)
    /// and reports `(page_count, laid char prefix, page cap tripped)`.
    pub fn finish(&mut self, emit: &mut dyn FnMut(LaidPage)) -> (u32, u64, bool) {
        if !self.capped && (self.has_content() || self.pages == 0) {
            self.flush(emit);
        }
        (self.pages, self.page_start, self.capped)
    }
}
