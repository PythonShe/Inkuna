import Foundation

/// Href handling shared by every screen that resolves the core's TOC and
/// in-book links against a reader session.
enum ChapterHref {
    /// Splits a chapter href into its resource path and optional fragment.
    static func splitFragment(_ href: String) -> (resource: String, fragment: String?) {
        guard let hashIndex = href.firstIndex(of: "#") else { return (href, nil) }
        return (
            String(href[..<hashIndex]),
            String(href[href.index(after: hashIndex)...])
        )
    }

    /// Href comparison key: fragment off, leading slash off, percent-decoded
    /// — so equivalent package-relative hrefs meet in the middle (including
    /// CJK resource names).
    static func normalized(_ href: String) -> String {
        var resource = splitFragment(href).resource
        if resource.hasPrefix("/") {
            resource.removeFirst()
        }
        return resource.removingPercentEncoding ?? resource
    }
}
