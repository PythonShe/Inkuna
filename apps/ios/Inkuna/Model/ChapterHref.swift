import Foundation

/// Href handling shared by every screen that joins the core's TOC with a
/// Readium locator or reading order.
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
    /// — so the core's package-root-relative hrefs and Readium's normalized
    /// link hrefs meet in the middle (CJK resource names included).
    static func normalized(_ href: String) -> String {
        var resource = splitFragment(href).resource
        if resource.hasPrefix("/") {
            resource.removeFirst()
        }
        return resource.removingPercentEncoding ?? resource
    }
}
