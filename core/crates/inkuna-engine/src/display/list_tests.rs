use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::dom::{parse, StylesheetSource};
use crate::fixed::Fx;
use crate::fonts::FontRegistry;
use crate::paginate::{paginate, ChapterInput, FxSize};
use crate::settings::LayoutSettings;
use crate::style::{parse_sheet, resolve};
use crate::text::project;

use super::{build_page, ColorRole, DisplayContext};

fn registry() -> &'static FontRegistry {
    static REG: OnceLock<Arc<FontRegistry>> = OnceLock::new();
    REG.get_or_init(|| {
        let dir = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../assets/fonts"
        ));
        FontRegistry::load(dir).expect("repo font set must load")
    })
}

fn ruby_positions(position: &str, vertical: bool) -> (f32, f32) {
    let writing_mode = if vertical {
        "body { writing-mode: vertical-rl; }"
    } else {
        ""
    };
    let src = format!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title>
<style>{writing_mode} ruby {{ ruby-position: {position}; }}</style></head>
<body><p><ruby>漢字<rt>かんじ</rt></ruby></p></body></html>"#
    );
    let doc = parse(src.as_bytes()).expect("fixture parses");
    let css: Vec<&str> = doc
        .stylesheets
        .iter()
        .filter_map(|sheet| match sheet {
            StylesheetSource::Inline(text) => Some(text.as_str()),
            StylesheetSource::Linked(_) => None,
        })
        .collect();
    let sheets: Vec<_> = css.iter().map(|sheet| parse_sheet(sheet)).collect();
    let styled = resolve(&doc, &sheets);
    let projection = project(&styled);
    let settings = LayoutSettings::default();
    let typography = settings.typography();
    let viewport = FxSize {
        width: Fx::from_pt(390.0),
        height: Fx::from_pt(664.0),
    };
    let input = ChapterInput {
        styled: &styled,
        projection: &projection,
        fonts: registry(),
        typography: &typography,
        settings: &settings,
        viewport,
        lang: None,
        resource_path: "OEBPS/ch01.xhtml",
        resources: &|_| None,
    };
    let ctx = DisplayContext::new(
        0,
        &styled,
        &projection,
        registry(),
        viewport,
        "OEBPS/ch01.xhtml",
    );
    let mut lists = Vec::new();
    paginate(&input, &mut |page| lists.push(build_page(&page, &ctx)))
        .expect("pagination is infallible");
    let (list, _) = lists.first().expect("one page");
    let base = list
        .glyph_runs
        .iter()
        .find(|run| run.color_role == ColorRole::Text)
        .expect("base run");
    let annotation = list
        .glyph_runs
        .iter()
        .find(|run| run.color_role == ColorRole::Secondary)
        .expect("annotation run");
    let cross_axis = if vertical { 0 } else { 1 };
    (base.positions[cross_axis], annotation.positions[cross_axis])
}

#[test]
fn ruby_annotations_follow_the_requested_cross_axis_side() {
    let (base_over, annotation_over) = ruby_positions("over", false);
    assert!(annotation_over < base_over, "over ruby sits above the base");

    let (base_under, annotation_under) = ruby_positions("under", false);
    assert!(
        annotation_under > base_under,
        "under ruby sits below the base"
    );

    let (base_over, annotation_over) = ruby_positions("over", true);
    assert!(
        annotation_over > base_over,
        "over ruby sits right of the vertical column"
    );

    let (base_under, annotation_under) = ruby_positions("under", true);
    assert!(
        annotation_under < base_under,
        "under ruby sits left of the vertical column"
    );
}
