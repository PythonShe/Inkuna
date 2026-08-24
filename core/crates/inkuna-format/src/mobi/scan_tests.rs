use super::scan_markup;

#[test]
fn repeated_target_at_the_distinct_target_cap_is_still_accepted() {
    let mut markup = String::new();
    for target in 0..100_000 {
        markup.push_str(&format!("<a filepos={target}></a>"));
    }
    markup.push_str("<a filepos=0></a>");

    let scan = scan_markup(markup.as_bytes()).unwrap();
    assert_eq!(scan.targets.len(), 100_000);
}
