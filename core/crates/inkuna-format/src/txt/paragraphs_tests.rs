use super::*;

#[test]
fn long_paragraph_parts_end_at_a_space_or_terminator_and_roundtrip() {
    let text = "这是一段没有任何换行的超长文字。An English sentence follows it!".repeat(2_000);
    assert!(text.len() > MAX_CHAPTER_TEXT_BYTES);
    let parts = split_long_paragraph(&text);
    assert!(parts.len() >= 2);
    for part in &parts {
        assert!(part.len() <= MAX_CHAPTER_TEXT_BYTES);
        let last = part.chars().next_back().unwrap();
        assert!(
            matches!(last, ' ' | '.' | '!' | '?' | '。' | '！' | '？' | '；'),
            "part ends with {last:?}"
        );
    }
    assert_eq!(parts.concat(), text);
}

#[test]
fn long_paragraph_split_falls_back_to_the_cap_without_any_cut_point() {
    let text = "无标点长文".repeat(20_000);
    assert!(text.len() > MAX_CHAPTER_TEXT_BYTES);
    let parts = split_long_paragraph(&text);
    assert!(parts.len() >= 2);
    assert!(parts
        .iter()
        .all(|part| part.len() <= MAX_CHAPTER_TEXT_BYTES));
    assert_eq!(parts.concat(), text);
}
