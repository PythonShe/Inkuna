use super::decode_entities;

#[test]
fn resolves_html5_and_numeric_entities_without_losing_unknown_references() {
    assert_eq!(
        decode_entities("A&nbsp;&bogus;&mdash;&#x4e2d;&#25991;"),
        "A\u{a0}&bogus;—中文"
    );
}

#[test]
fn preserves_many_unterminated_ampersands() {
    let input = "&".repeat(10_000);
    assert_eq!(decode_entities(&input), input);
}

#[test]
fn bare_ampersand_does_not_swallow_the_next_valid_entity() {
    assert_eq!(decode_entities("A & B &amp; C"), "A & B & C");
}
