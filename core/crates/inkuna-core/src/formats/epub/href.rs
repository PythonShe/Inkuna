//! Path resolution: every href a book carries is normalized to a
//! package-root-relative path before it is stored.

use percent_encoding::percent_decode_str;

pub(super) fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
}

/// Resolves `href` against the **document** that references it (TOC hrefs
/// are relative to the nav doc / NCX, not the package root). A
/// fragment-only href points into the referencing document itself.
pub(super) fn resolve_relative(doc_path: &str, href: &str) -> String {
    if let Some(fragment) = href.strip_prefix('#') {
        return format!("{doc_path}#{fragment}");
    }
    resolve_href(parent_dir(doc_path), href)
}

/// Resolves `href` (relative to `base_dir`, possibly percent-encoded,
/// possibly carrying a fragment) into a normalized package-root-relative
/// path, keeping the fragment verbatim.
pub(super) fn resolve_href(base_dir: &str, href: &str) -> String {
    let (path, fragment) = match href.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (href, None),
    };
    let decoded = percent_decode_str(path).decode_utf8_lossy();

    let mut segments: Vec<&str> = if decoded.starts_with('/') {
        Vec::new()
    } else {
        base_dir.split('/').filter(|s| !s.is_empty()).collect()
    };
    for segment in decoded.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            s => segments.push(s),
        }
    }
    let mut resolved = segments.join("/");
    if let Some(fragment) = fragment {
        resolved.push('#');
        resolved.push_str(fragment);
    }
    resolved
}

#[cfg(test)]
#[path = "href_tests.rs"]
mod tests;
