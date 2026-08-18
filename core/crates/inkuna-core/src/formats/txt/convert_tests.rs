use std::fs::File;
use std::io::Read;

use super::*;
use crate::formats::epub::{extract_spine_text, read_package};
use crate::CoreError;

fn convert(bytes: &[u8], title: &str) -> (tempfile::TempDir, std::path::PathBuf, TxtConversion) {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("source.txt");
    let dst = dir.path().join("book.epub");
    std::fs::write(&src, bytes).unwrap();
    let conversion = convert_to_epub(&src, &dst, title).unwrap();
    (dir, dst, conversion)
}

fn all_text(path: &std::path::Path) -> String {
    let package = read_package(path).unwrap();
    extract_spine_text(path, &package.spine)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn converts_cjk_chapters_to_nested_epub_toc() {
    let text =
        "第一卷 风起\n第一章 山中\n松风入夜。\n第二章 城外\n月照长街。\n第三章 归途\n故人归来。";
    let (_dir, dst, conversion) = convert(text.as_bytes(), "月光书房");
    assert_eq!(conversion.encoding, "UTF-8");

    let package = read_package(&dst).unwrap();
    assert_eq!(package.metadata.title.as_deref(), Some("月光书房"));
    assert_eq!(package.metadata.language.as_deref(), Some("zh"));
    assert_eq!(package.toc[0].title, "第一卷 风起");
    assert_eq!(package.toc[0].depth, 0);
    assert_eq!(package.toc[1].title, "第一章 山中");
    assert_eq!(package.toc[1].depth, 1);
    assert!(all_text(&dst).contains("松风入夜。"));
}

#[test]
fn falls_back_to_ten_kilobyte_parts_without_headings() {
    let paragraph = format!("{}。\n\n", "无题正文".repeat(400));
    let text = paragraph.repeat(5);
    let (_dir, dst, _) = convert(text.as_bytes(), "无题");
    let package = read_package(&dst).unwrap();
    assert!(package.spine.len() >= 2);
    assert_eq!(package.toc[0].title, "第1部分");
    assert_eq!(package.toc[1].title, "第2部分");
}

#[test]
fn handles_cr_only_and_fullwidth_indented_paragraphs() {
    let text = "第一章 起\r　第一段。\r续行。\r　第二段。\r续行。\r第二章 承\r　第三段。\r第三章 转\r　第四段。";
    let (_dir, dst, _) = convert(text.as_bytes(), "排版本");
    let body = all_text(&dst);
    assert!(body.contains("第一段。 续行。"), "{body}");
    assert!(body.contains("第二段。 续行。"), "{body}");
    assert!(!body.contains('\u{3000}'));
}

#[test]
fn emits_scene_breaks_for_marker_lines_and_three_blank_lines() {
    let text = "第一章 起\n山风吹过。\n＊＊＊\n月色正明。\n\n\n\n故人归来。\n第二章 承\n长街寂静。\n第三章 转\n城门已开。";
    let (_dir, dst, _) = convert(text.as_bytes(), "场景");
    let file = File::open(&dst).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut xhtml = String::new();
    archive
        .by_name("OEBPS/text/ch00001.xhtml")
        .unwrap()
        .read_to_string(&mut xhtml)
        .unwrap();
    assert_eq!(xhtml.matches("<hr class=\"scene\"/>").count(), 2, "{xhtml}");
    assert!(!xhtml.contains("<p></p>"));
}

#[test]
fn splits_oversized_chapters_at_paragraph_boundaries() {
    let large_body = (0..1800)
        .map(|index| format!("第{index}段{}。", "长文".repeat(20)))
        .collect::<Vec<_>>()
        .join("\n\n");
    let text = format!("第一章 巨篇\n{large_body}\n第二章 次篇\n短文。\n第三章 末篇\n终。");
    let (_dir, dst, _) = convert(text.as_bytes(), "长篇");
    let package = read_package(&dst).unwrap();
    assert!(package
        .toc
        .iter()
        .any(|entry| entry.title == "第一章 巨篇 (2)"));
    assert!(package.spine.len() > 3);
}

#[test]
fn splits_one_oversized_unbroken_paragraph() {
    let large_body = "长文".repeat(20_000);
    let text = format!("第一章 巨篇\n{large_body}\n第二章 次篇\n短文。\n第三章 末篇\n终。");
    let (_dir, dst, _) = convert(text.as_bytes(), "长篇");
    let package = read_package(&dst).unwrap();
    assert!(package
        .toc
        .iter()
        .any(|entry| entry.title == "第一章 巨篇 (2)"));
}

#[test]
fn recognizes_scene_markers_in_western_line_groups() {
    let text = "Chapter 1 Dawn\nFirst line.\n***\nSecond line.\nChapter 2 Noon\nThird line.\nChapter 3 Night\nFourth line.";
    let (_dir, dst, _) = convert(text.as_bytes(), "Scenes");
    let file = File::open(&dst).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut xhtml = String::new();
    archive
        .by_name("OEBPS/text/ch00001.xhtml")
        .unwrap()
        .read_to_string(&mut xhtml)
        .unwrap();
    assert!(xhtml.contains("<hr class=\"scene\"/>"), "{xhtml}");
}

#[test]
fn rejects_empty_or_whitespace_only_text() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("empty.txt");
    let dst = dir.path().join("empty.epub");
    std::fs::write(&src, " \t\r\n　").unwrap();
    let error = convert_to_epub(&src, &dst, "空").unwrap_err();
    assert!(matches!(error, CoreError::InvalidPublication(_)));
    assert!(!dst.exists());
}

#[test]
fn rejects_decoded_text_past_the_aggregate_parser_budget() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("oversized.txt");
    let dst = dir.path().join("oversized.epub");
    std::fs::write(&src, vec![b'a'; MAX_TOTAL_TEXT_BYTES + 1]).unwrap();
    let error = convert_to_epub(&src, &dst, "Oversized").unwrap_err();
    assert!(
        matches!(error, CoreError::InvalidPublication(message) if message.contains("decoded plain text"))
    );
    assert!(!dst.exists());
}

#[test]
fn does_not_label_mostly_latin_utf8_as_chinese_for_one_han_character() {
    let text = format!(
        "One Han character 字 in otherwise Latin prose. {}",
        "word ".repeat(50)
    );
    let (_dir, dst, _) = convert(text.as_bytes(), "Language");
    let package = read_package(&dst).unwrap();
    assert_eq!(package.metadata.language.as_deref(), Some("und"));
}

#[test]
fn escapes_plain_text_before_embedding_it_in_xhtml() {
    let text =
        "Chapter 1 Dawn\n月 < 星 & 夜。\nChapter 2 Noon\nD > C.\nChapter 3 Night\n\"quoted\"。";
    let (_dir, dst, _) = convert(text.as_bytes(), "Escaping");
    let body = all_text(&dst);
    assert!(body.contains("月 < 星 & 夜。"), "{body}");
    assert!(body.contains("\"quoted\"。"), "{body}");
}

#[test]
fn coalesces_across_volume_boundaries_to_the_txt_spine_budget() {
    let mut sections = (0..=MAX_TXT_CHAPTERS)
        .flat_map(|index| {
            [
                OutputSection::Volume(format!("第{index}卷")),
                OutputSection::Chapter {
                    title: format!("第{index}章"),
                    xhtml: format!("<p>{index}</p>"),
                },
            ]
        })
        .collect::<Vec<_>>();
    coalesce_chapters(&mut sections);
    let count = sections
        .iter()
        .filter(|section| matches!(section, OutputSection::Chapter { .. }))
        .count();
    assert!(count <= MAX_TXT_CHAPTERS);
    assert!(matches!(
        &sections[0],
        OutputSection::Volume(title) if title == "第0卷"
    ));
    assert!(matches!(
        &sections[1],
        OutputSection::Chapter { title, xhtml }
            if title == "第0章"
                && xhtml.contains("<p>0</p><h2>第1卷</h2><p>1</p>")
    ));
}
