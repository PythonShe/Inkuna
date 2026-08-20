import Foundation
import Synchronization

/// The reader's own user stylesheet — the one styling path that survives
/// dropping the Readium navigator.
///
/// A value snapshot of the six Customize settings, rendered to CSS that is
/// spliced into every resource at serve time (see
/// `ReaderViewController.injectingInkunaStyles`) and pushed into already
/// loaded spreads by `applyScript(mode:)`. Theme, text size, and
/// brightness stay on the navigator's preferences; nothing here touches
/// them.
struct ReaderUserStyle: Equatable {
    var font: ReadingFont
    /// The stored font id verbatim — `font.rawValue` for anything this
    /// build knows, and the untouched original for anything it does not.
    /// Persisting this instead of `font.rawValue` keeps an unknown id
    /// intact through commits that are not font picks.
    var fontID: String = ReadingFont.publisher.rawValue
    var bold: Bool
    var lineSpacing: Double
    var letterSpacing: Double
    var wordSpacing: Double
    var margins: Int

    static let styleElementID = "inkuna-user"

    /// The committed settings as a snapshot.
    @MainActor static var current: ReaderUserStyle {
        let settings = AppSettings.shared
        return ReaderUserStyle(
            font: settings.readingFont,
            fontID: settings.readingFontID,
            bold: settings.readingBold,
            lineSpacing: settings.lineSpacing,
            letterSpacing: settings.letterSpacing,
            wordSpacing: settings.wordSpacing,
            margins: settings.readingMargins
        )
    }

    /// Persists this snapshot as the committed settings.
    @MainActor func persist() {
        let settings = AppSettings.shared
        settings.readingFontID = fontID
        settings.readingBold = bold
        settings.lineSpacing = lineSpacing
        settings.letterSpacing = letterSpacing
        settings.wordSpacing = wordSpacing
        settings.readingMargins = margins
    }

    /// The full stylesheet. Every rule is `html`-rooted and `!important`.
    /// ReadiumCSS's competing typography rules are `:root` — specificity
    /// (0,1,0), which would beat our (0,0,2) — but every one of them is
    /// gated on a `--USER__*` preference this app never submits, so they
    /// stay inert. Invariant: never route line-height/letter/word-spacing/
    /// font-family through `EPUBPreferences`, or ReadiumCSS wins silently.
    /// ASCII-only and free of `</`, so the sheet is safe both spliced into
    /// HTML and quoted into a JS literal.
    func css() -> String {
        // Non-localized formatting: a de-locale device must not emit 1,65.
        func num(_ value: Double) -> String {
            String(format: "%.4g", locale: Locale(identifier: "en_US_POSIX"), value)
        }
        var rules: [String] = []

        // The values as custom properties: the forward contract the future
        // renderer (and any diagnostics) can read back off the page.
        rules.append(
            ":root{--inkuna-line-height:\(num(lineSpacing));"
                + "--inkuna-letter-spacing:\(num(letterSpacing))em;"
                + "--inkuna-word-spacing:\(num(wordSpacing))em;"
                + "--inkuna-margins:\(margins)px;"
                + "--inkuna-font-weight:\(bold ? 600 : 400)}"
        )

        // Margins: body is the fragmented child of the :root multicol, so
        // inline-axis padding repeats inside every column and never moves
        // the column grid the pager measures. Logical properties, because in
        // vertical-rl the physical left/right become the fragmentation axis,
        // where sliced padding lands only on the first and last page.
        rules.append(
            "html body{padding-inline-start:var(--inkuna-margins)!important;"
                + "padding-inline-end:var(--inkuna-margins)!important}"
        )

        // Line height: root value + explicit inherit on the blocks a
        // publisher may have styled directly. Headings keep their own.
        rules.append("html{line-height:var(--inkuna-line-height)!important}")
        rules.append(
            "html body,html p,html li,html div,html dd,html blockquote"
                + "{line-height:inherit!important}"
        )

        let spacedBlocks = "html h1,html h2,html h3,html h4,html h5,html h6,"
            + "html p,html li,html div,html dd,html blockquote,html td"
        if letterSpacing > 0 {
            rules.append(
                "\(spacedBlocks){letter-spacing:var(--inkuna-letter-spacing)!important;"
                    + "font-variant:none!important}"
            )
        }
        if wordSpacing > 0 {
            rules.append("\(spacedBlocks){word-spacing:var(--inkuna-word-spacing)!important}")
        }

        // Bold: body text only. b/strong/th/headings are left alone so the
        // publisher's own emphasis (700+) stays heavier than the text.
        if bold {
            rules.append(
                "html body,html p,html li,html div,html dd,html blockquote,html td"
                    + "{font-weight:600!important}"
            )
        }

        // Font family: the ReadiumCSS-after selector set, including the
        // :not([lang]) guards that keep inline foreign-language runs on
        // their own face. `.publisher` emits nothing at all.
        if let stack = font.cssStack {
            rules.append("html{font-family:\(stack)!important}")
            rules.append(
                "html body,html p,html li,html div,html dt,html dd"
                    + "{font-family:inherit!important}"
            )
            rules.append(
                "html i:not([lang]):not([xml\\:lang]),html em:not([lang]):not([xml\\:lang]),"
                    + "html cite:not([lang]):not([xml\\:lang]),html b:not([lang]):not([xml\\:lang]),"
                    + "html strong:not([lang]):not([xml\\:lang]),html span:not([lang]):not([xml\\:lang])"
                    + "{font-family:inherit!important}"
            )
        }

        return rules.joined()
    }

    /// How the apply script treats the reading position across the reflow.
    enum AnchorMode: String {
        /// Capture a fresh anchor (an interaction is starting).
        case begin
        /// Re-use the anchor captured at `begin` (mid-drag preview).
        case live
        /// Re-land and release the anchor (the interaction committed).
        case end
    }

    /// One self-contained, idempotent JS statement: installs the
    /// stylesheet (last in head, so live updates and the serve-time splice
    /// converge on one element), then re-lands the page on a column
    /// boundary using a text anchor captured at the interaction's start.
    func applyScript(mode: AnchorMode) -> String {
        let cssLiteral = Self.jsStringLiteral(css())
        return """
        (function(css,mode){
        var d=document,root=d.documentElement;
        if(!d.body)return;
        if(mode==='begin'||!window.__inkunaAnchor){
        window.__inkunaAnchor=(function(){
        var w=d.createTreeWalker(d.body,NodeFilter.SHOW_TEXT,null),n,W=window.innerWidth;
        while((n=w.nextNode())){
        if(!n.nodeValue.trim())continue;
        var r=d.createRange();r.selectNodeContents(n);
        var b=r.getBoundingClientRect();
        if(b.width>0&&b.right>0&&b.left<W)return n.parentElement;
        }
        return null;
        })();
        var el=d.scrollingElement;
        window.__inkunaFallback=el.scrollWidth>0?Math.abs(el.scrollLeft)/el.scrollWidth:0;
        }
        var s=d.getElementById('\(Self.styleElementID)');
        if(!s){s=d.createElement('style');s.id='\(Self.styleElementID)';}
        if(s.parentNode!==d.head||d.head.lastElementChild!==s)d.head.appendChild(s);
        s.textContent=css;
        requestAnimationFrame(function(){requestAnimationFrame(function(){
        var el=d.scrollingElement,
        rtl=getComputedStyle(root).direction==='rtl',
        W=window.innerWidth,sign=rtl?-1:1,target;
        var a=window.__inkunaAnchor;
        if(a&&a.isConnected){
        var b=a.getBoundingClientRect();
        target=Math.abs(el.scrollLeft)+sign*b.left;
        }else{
        target=(window.__inkunaFallback||0)*el.scrollWidth;
        }
        if(W>0)target=Math.round(target/W)*W;
        target=Math.max(0,Math.min(target,Math.max(0,el.scrollWidth-W)));
        el.scrollLeft=sign*target;
        if(mode==='end'){delete window.__inkunaAnchor;delete window.__inkunaFallback;}
        })});
        })(\(cssLiteral),'\(mode.rawValue)');
        """
    }

    /// Escapes a string into a double-quoted JS literal. `<` is escaped so
    /// the result can never form `</script>` inside markup contexts.
    static func jsStringLiteral(_ string: String) -> String {
        var out = "\""
        for scalar in string.unicodeScalars {
            switch scalar {
            case "\\": out += "\\\\"
            case "\"": out += "\\\""
            case "\n": out += "\\n"
            case "\r": out += "\\r"
            case "<": out += "\\u003C"
            default: out.unicodeScalars.append(scalar)
            }
        }
        return out + "\""
    }
}

/// The live stylesheet, written on the main actor when settings change and
/// read by the container transform on whatever thread serves a resource —
/// so a spread preloaded after a settings change paints correctly instead
/// of flashing stale typography.
final class ReaderUserStyleBox: Sendable {
    private let css = Mutex<String>("")

    func read() -> String { css.withLock { $0 } }
    func write(_ new: String) { css.withLock { $0 = new } }
}
