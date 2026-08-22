use super::*;

fn chapter_titles(sections: &[DetectedSection]) -> Vec<&str> {
    sections
        .iter()
        .filter_map(|section| match section {
            DetectedSection::Chapter { title, .. } => Some(title.as_str()),
            DetectedSection::Volume { .. } => None,
        })
        .collect()
}

#[test]
fn detects_cjk_numeral_fullwidth_and_spacing_variants() {
    let text = "第1024章 千\n山风吹过。\n第三千二百一十六章 万\n月色正明。\n第０１２章 零\n故人归来。\n第 12 章 空\n灯火阑珊。";
    let sections = detect_sections(text).unwrap();
    assert_eq!(
        chapter_titles(&sections),
        [
            "第1024章 千",
            "第三千二百一十六章 万",
            "第０１２章 零",
            "第 12 章 空"
        ]
    );
}

#[test]
fn does_not_split_on_a_chapter_reference_inside_a_paragraph() {
    let text =
        "第一章 起\n「他想起第三章说过的话。」\n第二章 承\n山路渐远。\n第三章 转\n城门已开。";
    let sections = detect_sections(text).unwrap();
    assert_eq!(
        chapter_titles(&sections),
        ["第一章 起", "第二章 承", "第三章 转"]
    );
    match &sections[0] {
        DetectedSection::Chapter { body, .. } => assert!(body.contains("第三章说过的话")),
        other => panic!("expected chapter, got {other:?}"),
    }
}

#[test]
fn detects_unnumbered_cjk_headings() {
    let text = "序章\n开端\n楔子\n前情\n番外\n别传\n终章\n结局";
    let sections = detect_sections(text).unwrap();
    assert_eq!(chapter_titles(&sections), ["序章", "楔子", "番外", "终章"]);
}

#[test]
fn folds_a_volume_marker_before_its_first_chapter() {
    let text =
        "第一卷 风起\n第一章 山中\n松风入夜。\n第二章 城外\n月照长街。\n第三章 归途\n故人归来。";
    let sections = detect_sections(text).unwrap();
    assert!(matches!(
        &sections[0],
        DetectedSection::Volume { title } if title == "第一卷 风起"
    ));
    assert_eq!(
        chapter_titles(&sections),
        ["第一章 山中", "第二章 城外", "第三章 归途"]
    );
}

#[test]
fn folds_blank_toc_dump_headings_instead_of_creating_empty_chapters() {
    let text = "第一章 风\n第二章 雨\n第三章 雷\n第一章 风\n山风吹过。\n第二章 雨\n细雨连绵。\n第三章 雷\n雷声渐远。";
    let sections = detect_sections(text).unwrap();
    assert_eq!(
        chapter_titles(&sections),
        ["第一章 风", "第二章 雨", "第三章 雷"]
    );
    assert!(sections[..3]
        .iter()
        .all(|section| matches!(section, DetectedSection::Volume { .. })));
}

#[test]
fn detects_western_numeric_separator_and_bracketed_rules() {
    let western = detect_sections(
        "Prologue\nOpening\nChapter 1 Dawn\nOne\nCHAPTER II Noon\nTwo\nEpilogue\nEnd",
    )
    .unwrap();
    assert_eq!(
        chapter_titles(&western),
        ["Prologue", "Chapter 1 Dawn", "CHAPTER II Noon", "Epilogue"]
    );

    let numeric = detect_sections("1：起点\n甲\n2、途中\n乙\n3—终点\n丙").unwrap();
    assert_eq!(chapter_titles(&numeric), ["1：起点", "2、途中", "3—终点"]);

    let bracketed =
        detect_sections("【第1章 起】\n甲\n〔Chapter 2节 承〕\n乙\n［第３话 转］\n丙").unwrap();
    assert_eq!(
        chapter_titles(&bracketed),
        ["【第1章 起】", "〔Chapter 2节 承〕", "［第３话 转］"]
    );
}

#[test]
fn applies_legado_suffix_guards() {
    let text = "序章\n开端\n第1节课 数学\n仍在序章\n第一章 正文\n第一回合 比赛\n仍在第一章\n第二章 继续\n完";
    let sections = detect_sections(text).unwrap();
    assert_eq!(
        chapter_titles(&sections),
        ["序章", "第一章 正文", "第二章 继续"]
    );
    match &sections[0] {
        DetectedSection::Chapter { body, .. } => assert!(body.contains("第1节课 数学")),
        other => panic!("expected chapter, got {other:?}"),
    }
}

#[test]
fn requires_three_competing_rule_matches_and_titles_a_cjk_preface() {
    assert!(detect_sections("第一章 起\n甲。\n第二章 承\n乙。")
        .unwrap()
        .is_empty());

    let sections =
        detect_sections("很久以前。\n第一章 起\n甲。\n第二章 承\n乙。\n第三章 转\n丙。").unwrap();
    assert!(matches!(
        &sections[0],
        DetectedSection::Chapter { title, body } if title == "前言" && body == "很久以前。"
    ));
}
