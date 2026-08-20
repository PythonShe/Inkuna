package app.inkuna.android.ui.reader

import android.webkit.WebView
import androidx.annotation.MainThread
import androidx.webkit.ScriptHandler
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import org.json.JSONObject
import java.lang.ref.WeakReference

/**
 * Owns the reader's own user stylesheet — the one styling path that
 * survives dropping Readium's navigator.
 *
 * Every resource WebView gets a document-start script that plants
 * `<style id="inkuna-user">` before the first layout pass (no flash of
 * publisher typography), re-asserted at DOMContentLoaded so the element
 * ends up last in `<head>` — after ReadiumCSS — where it wins cascade
 * ties. Already-loaded documents are updated in place through the same
 * hook. Registration is per WebView, so it survives every navigation
 * that WebView performs afterwards.
 */
@MainThread
class ReaderStyleInjector {

    private class Entry(val ref: WeakReference<WebView>, var handler: ScriptHandler?)

    private val entries = mutableListOf<Entry>()
    private var css: String = ""

    /** Called once per new resource WebView (ReaderWebViewTuner). */
    fun register(webView: WebView) {
        prune()
        if (entries.any { it.ref.get() === webView }) return
        val entry = Entry(WeakReference(webView), null)
        entries.add(entry)
        arm(entry)
    }

    fun unregister(webView: WebView) {
        entries.removeAll { entry ->
            val view = entry.ref.get()
            if (view == null || view === webView) {
                entry.handler?.remove()
                true
            } else {
                false
            }
        }
    }

    /** Installs `css` into every live document, current and future. */
    fun setCss(new: String) {
        if (new == css) return
        css = new
        prune()
        entries.forEach(::arm)
    }

    /** The live resource WebViews, most recently registered last. */
    fun webViews(): List<WebView> {
        prune()
        return entries.mapNotNull { it.ref.get() }
    }

    fun detach() {
        for (entry in entries) entry.handler?.remove()
        entries.clear()
    }

    private fun arm(entry: Entry) {
        val view = entry.ref.get() ?: return
        entry.handler?.remove()
        if (WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            entry.handler = WebViewCompat.addDocumentStartJavaScript(
                view,
                bootstrap(css),
                setOf("*"),
            )
        }
        // Catch up the document already loaded in this WebView — also the
        // whole degraded path on a WebView without document-start support
        // (a possible one-frame flash, never a crash).
        view.evaluateJavascript(update(css), null)
    }

    private fun prune() {
        entries.removeAll { entry ->
            val view = entry.ref.get()
            if (view == null || !view.isAttachedToWindow) {
                entry.handler?.remove()
                true
            } else {
                false
            }
        }
    }

    private fun bootstrap(css: String): String = """
        (function () {
          var ID = 'inkuna-user';
          var css = ${JSONObject.quote(css)};
          function ensure() {
            var el = document.getElementById(ID);
            if (!el) {
              el = document.createElement('style');
              el.id = ID;
              el.setAttribute('type', 'text/css');
            }
            if (el.textContent !== css) el.textContent = css;
            // Head when it exists, documentElement before it is parsed: a
            // style element applies from wherever it sits, and moving it
            // last in head once parsed puts it after ReadiumCSS-after.css
            // in the cascade.
            var host = document.head || document.documentElement;
            if (el.parentNode !== host || host.lastChild !== el) host.appendChild(el);
          }
          window.__inkunaSetUserCss = function (next) { css = next; ensure(); };
          ensure();
          if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', ensure, { once: true });
          }
        })();
    """.trimIndent()

    private fun update(css: String): String =
        "if(window.__inkunaSetUserCss)window.__inkunaSetUserCss(${JSONObject.quote(css)});"
}
