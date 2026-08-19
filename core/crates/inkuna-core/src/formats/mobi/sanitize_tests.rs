use super::{safe_href, safe_image_src};

#[test]
fn relative_references_with_colons_after_a_delimiter_are_safe() {
    assert!(safe_href("notes/a:b.html"));
    assert!(safe_href("page.html?q=a:b"));
    assert!(safe_href("page.html#a:b"));
    assert!(safe_image_src("img/a:b.png"));
    assert!(safe_image_src("a.png?v=1:2"));
}

#[test]
fn allowlisted_schemes_pass_and_dangerous_ones_are_rejected() {
    assert!(safe_href("http://example.com/x"));
    assert!(safe_href("HTTPS://example.com/x"));
    assert!(safe_href("mailto:someone@example.com"));
    assert!(!safe_href("javascript:alert(1)"));
    assert!(!safe_href("data:text/html,x"));
    assert!(!safe_href("vbscript:x"));
}

#[test]
fn protocol_relative_and_absolute_paths_are_rejected() {
    assert!(!safe_href("//evil/x"));
    assert!(!safe_image_src("//evil/x"));
    assert!(!safe_image_src("/abs/x"));
    assert!(safe_href("/abs/x"));
}

#[test]
fn image_sources_never_carry_a_scheme() {
    assert!(!safe_image_src("http://example.com/x.png"));
    assert!(!safe_image_src("data:image/png;base64,x"));
    assert!(safe_image_src("cover.png"));
}

#[test]
fn empty_and_scheme_edge_cases() {
    assert!(!safe_href(""));
    assert!(!safe_href("   "));
    // A one-letter scheme is still a scheme per RFC 3986.
    assert!(!safe_href("a:b/c"));
    // A candidate that fails the scheme grammar is not a scheme at all.
    assert!(safe_href("1:2"));
    assert!(safe_href("a b:c"));
}
