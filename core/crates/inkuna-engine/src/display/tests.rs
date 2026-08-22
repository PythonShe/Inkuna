use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::dom::{parse, StylesheetSource};
use crate::fixed::Fx;
use crate::fonts::FontRegistry;
use crate::paginate::{paginate, ChapterInput, FxSize};
use crate::settings::LayoutSettings;
use crate::style::{parse_sheet, resolve};
use crate::test_support::{CJK_VERTICAL_RUBY_DOC, LATIN_DOC};
use crate::text::project;

use super::{build_page, A11yRole, ColorRole, DecorationKind, DisplayContext, PageDisplayList, PageMaps};

fn registry() -> &'static FontRegistry {
    static REG: OnceLock<Arc<FontRegistry>> = OnceLock::new();
    REG.get_or_init(|| {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets/fonts"));
        FontRegistry::load(dir).expect("repo font set must load")
    })
}

const RESOURCE_PATH: &str = "OEBPS/ch01.xhtml";

/// Parses, styles, projects, paginates, and display-builds one document
/// at default settings.
fn build(doc_src: &str, w: f64, h: f64) -> Vec<(PageDisplayList, PageMaps)> {
    let doc = parse(doc_src.as_bytes()).expect("fixture parses");
    let css: Vec<&str> = doc
        .stylesheets
        .iter()
        .filter_map(|s| match s {
            StylesheetSource::Inline(text) => Some(text.as_str()),
            StylesheetSource::Linked(_) => None,
        })
        .collect();
    let sheets: Vec<_> = css.iter().map(|c| parse_sheet(c)).collect();
    let styled = resolve(&doc, &sheets);
    let projection = project(&styled);
    let settings = LayoutSettings::default();
    let typography = settings.typography();
    let viewport = FxSize {
        width: Fx::from_pt(w),
        height: Fx::from_pt(h),
    };
    let input = ChapterInput {
        styled: &styled,
        projection: &projection,
        fonts: registry(),
        typography: &typography,
        settings: &settings,
        viewport,
        lang: None,
        resource_path: RESOURCE_PATH,
        resources: &|_| None,
    };
    let ctx = DisplayContext::new(7, &styled, &projection, registry(), viewport, RESOURCE_PATH);
    let mut out = Vec::new();
    paginate(&input, &mut |page| out.push(build_page(&page, &ctx)))
        .expect("paginate is infallible");
    out
}

#[test]
fn positions_interleaved_len() {
    for doc in [LATIN_DOC, CJK_VERTICAL_RUBY_DOC] {
        let pages = build(doc, 390.0, 664.0);
        let mut runs = 0usize;
        for (list, _) in &pages {
            assert_eq!(list.generation, 7);
            for run in &list.glyph_runs {
                assert!(!run.glyph_ids.is_empty());
                assert_eq!(run.positions.len(), 2 * run.glyph_ids.len());
                runs += 1;
            }
        }
        assert!(runs > 0, "fixture must emit glyph runs");
    }
}

#[test]
fn link_regions_and_color() {
    let pages = build(LATIN_DOC, 390.0, 664.0);
    let (list, _) = &pages[0];
    // The internal fragment link normalizes against the chapter's own
    // path, fragment kept.
    let region = list
        .links
        .iter()
        .find(|l| l.target == "OEBPS/ch01.xhtml#top")
        .expect("link region for the fragment link");
    assert!(region.rect.width > 0.0 && region.rect.height > 0.0);
    assert!(
        list.glyph_runs
            .iter()
            .any(|r| r.color_role == ColorRole::Link),
        "link glyphs carry the Link role"
    );
    let underline = list
        .decorations
        .iter()
        .find(|d| d.kind == DecorationKind::Underline)
        .expect("underline under the link run");
    assert_eq!(underline.color_role, ColorRole::Link);
    // The underline sits inside the link region's inline stretch.
    assert!(underline.rect.x >= region.rect.x - 0.01);
    assert!(
        underline.rect.x + underline.rect.width <= region.rect.x + region.rect.width + 0.01
    );
    // Non-link text stays Text.
    assert!(list.glyph_runs.iter().any(|r| r.color_role == ColorRole::Text));
}

#[test]
fn ruby_is_secondary_and_parenthetical() {
    let pages = build(CJK_VERTICAL_RUBY_DOC, 390.0, 664.0);
    let (list, _) = &pages[0];
    assert!(
        list.glyph_runs
            .iter()
            .any(|r| r.color_role == ColorRole::Secondary),
        "ruby annotation runs are Secondary"
    );
    let block = list
        .a11y
        .iter()
        .find(|b| b.text.contains("東京"))
        .expect("the ruby paragraph's a11y block");
    assert_eq!(block.text, "東京（とうきょう）の空（そら）は高かった。");
}

#[test]
fn a11y_blocks_ordered_with_lang() {
    let doc = r#"<html xmlns="http://www.w3.org/1999/xhtml" lang="en"><head><title>t</title></head>
<body>
<h1>Title</h1>
<p lang="ja">日本語の文。</p>
<p>Plain body.</p>
</body></html>"#;
    let pages = build(doc, 390.0, 664.0);
    let (list, _) = &pages[0];
    assert_eq!(list.a11y.len(), 3);
    assert_eq!(list.a11y[0].text, "Title");
    assert_eq!(list.a11y[0].role, A11yRole::Heading);
    assert_eq!(list.a11y[0].lang.as_deref(), Some("en"));
    assert_eq!(list.a11y[1].text, "日本語の文。");
    assert_eq!(list.a11y[1].role, A11yRole::Body);
    assert_eq!(list.a11y[1].lang.as_deref(), Some("ja"));
    assert_eq!(list.a11y[2].text, "Plain body.");
    assert_eq!(list.a11y[2].lang.as_deref(), Some("en"));
    // Reading order: blocks descend the page.
    assert!(list.a11y[0].rect.y < list.a11y[1].rect.y);
    assert!(list.a11y[1].rect.y < list.a11y[2].rect.y);
}

#[test]
fn maps_cover_page_chars() {
    let mut body = String::new();
    for _ in 0..40 {
        body.push_str("<p>月光洒在窗台上，屋里一片寂静。他放下手中的书，望向远处的群山。</p>\n");
    }
    let doc = format!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body>{body}</body></html>"#
    );
    let pages = build(&doc, 200.0, 240.0);
    assert!(pages.len() > 1, "fixture must span multiple pages");
    for (_, maps) in &pages {
        let mut covered = maps.char_range.start;
        for line in &maps.lines {
            assert_eq!(line.char_range.start, covered, "lines tile the page range");
            covered = line.char_range.end;
            assert!(line.band.w >= Fx::ZERO && line.band.h >= Fx::ZERO);
        }
        assert_eq!(covered, maps.char_range.end);
    }
}
