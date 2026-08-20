package app.inkuna.android.ui.reader

import android.webkit.WebView
import app.inkuna.android.model.AppSettings
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.android.awaitFrame
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import org.json.JSONObject
import org.json.JSONTokener
import kotlin.coroutines.resume

/**
 * Drives the reader's own user stylesheet. Every Customize setting
 * applies through here — never through EpubPreferences — so the styling
 * pipeline outlives Readium's navigator; theme and text size stay on
 * submitPreferences.
 *
 * Reflow discipline: previews during a drag only rewrite CSS (cheap, per
 * step); the reading position is anchored once when the interaction
 * begins and re-landed once when it commits, followed by a pager
 * recalibration so the next gesture measures the new column grid.
 */
class ReaderAppearanceController(
    private val scope: CoroutineScope,
    private val injector: ReaderStyleInjector,
    private val pager: () -> ReaderPagerLayout?,
) {
    private var anchorJson: String? = null
    private var restoreJob: Job? = null

    /** A slider touch landed: capture the reflow anchor once. */
    fun beginPreview() {
        if (anchorJson != null) return
        val visible = pager()?.currentWebView() ?: return
        pager()?.cancelInteraction()
        visible.evaluateJavascript(CAPTURE_JS) { result ->
            anchorJson = result?.let(::decodeJsString)
        }
    }

    /** Mid-drag: CSS only — no restore, no re-seed. */
    fun preview(draft: ReaderTypeDraft) {
        injector.setCss(ReaderUserCss.build(draft))
    }

    /** The interaction committed: restore the anchor and recalibrate. */
    fun endPreview() {
        restoreAndRecalibrate()
    }

    /**
     * A committed snapshot arrived (font pick, bold, reset, or an external
     * change): apply and re-land in one pass. Anchors first if no preview
     * session already did.
     */
    fun applyCommitted(snapshot: AppSettings.Snapshot) {
        val css = ReaderUserCss.build(ReaderTypeDraft.from(snapshot))
        if (anchorJson == null) beginPreview()
        injector.setCss(css)
        restoreAndRecalibrate()
    }

    /** A phrase off the current page, for the Customize preview. */
    suspend fun currentPhrase(): String? =
        pager()?.currentWebView()?.let { ReaderPhraseProbe.currentPhrase(it) }

    private fun restoreAndRecalibrate() {
        restoreJob?.cancel()
        restoreJob = scope.launch {
            // Let the WebView reach the frame after the CSS lands, then
            // give Blink's column relayout a beat before re-landing.
            awaitFrame()
            delay(REFLOW_SETTLE_MS)
            val visible = pager()?.currentWebView()
            if (visible != null) {
                val anchor = anchorJson ?: "null"
                visible.evaluateJavascriptAndAwait("($RESTORE_JS)($anchor)")
            }
            anchorJson = null
            pager()?.recalibrate()
        }
    }

    private companion object {
        const val REFLOW_SETTLE_MS = 32L

        /**
         * Captures the first visible text element as a Readium locator
         * (precedented dependency: the pager already calls readium.* for
         * neighbour pre-positioning) plus a column-quantisable progression
         * fallback for documents where the locator probe fails.
         */
        val CAPTURE_JS = """
            (function () {
              var d = document.scrollingElement;
              var p = d && d.scrollWidth > 0 ? d.scrollLeft / d.scrollWidth : 0;
              var loc = null;
              try { loc = readium.findFirstVisibleLocator(); } catch (e) {}
              return JSON.stringify({ loc: loc, p: p });
            })()
        """.trimIndent()

        /**
         * Re-lands after the reflow: the locator's column when possible,
         * else the progression snapped to the column grid — never mid-page.
         */
        val RESTORE_JS = """
            function (a) {
              var d = document.scrollingElement;
              if (!d) return 0;
              if (a && a.loc) {
                try { readium.scrollToLocator(a.loc); return 1; } catch (e) {}
              }
              var w = d.clientWidth || 1;
              var x = Math.round(((a && a.p || 0) * d.scrollWidth) / w) * w;
              d.scrollLeft = Math.min(Math.max(0, x), Math.max(0, d.scrollWidth - w));
              return 0;
            }
        """.trimIndent()

        /** evaluateJavascript returns a JSON-encoded value; decode or null. */
        fun decodeJsString(encoded: String): String? =
            runCatching { JSONTokener(encoded).nextValue() as? String }.getOrNull()
                ?.takeIf { it.isNotBlank() && it != "null" }
    }
}

private suspend fun WebView.evaluateJavascriptAndAwait(script: String) {
    suspendCancellableCoroutine { continuation ->
        evaluateJavascript(script) { continuation.resume(Unit) }
    }
}

/** Extracts a phrase from the visible column of a resource WebView. */
object ReaderPhraseProbe {

    suspend fun currentPhrase(webView: WebView): String? =
        suspendCancellableCoroutine { continuation ->
            webView.evaluateJavascript(PHRASE_JS) { result ->
                val text = result
                    ?.let { runCatching { JSONTokener(it).nextValue() as? String }.getOrNull() }
                    ?.takeIf { it.isNotBlank() }
                continuation.resume(text)
            }
        }

    /**
     * Visible text nodes of the current column (centre-in-viewport rects,
     * so a line straddling the gutter counts for the page holding most of
     * it), split on terminators from every script we ship — never a
     * Latin-only rule — with a script-aware length window (CJK packs more
     * meaning per character and has no spaces).
     */
    private val PHRASE_JS = """
        (function () {
          var W = window.innerWidth, H = window.innerHeight;
          var body = document.body;
          if (!body) return '';
          var walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT, {
            acceptNode: function (n) {
              if (!n.nodeValue || !n.nodeValue.trim()) return NodeFilter.FILTER_REJECT;
              for (var p = n.parentNode; p && p.nodeType === 1; p = p.parentNode) {
                var t = p.tagName.toLowerCase();
                if (t === 'script' || t === 'style' || t === 'rt' || t === 'rp' ||
                    t === 'code' || t === 'pre' || t === 'sup' || t === 'sub') {
                  return NodeFilter.FILTER_REJECT;
                }
              }
              return NodeFilter.FILTER_ACCEPT;
            }
          });
          var parts = [], budget = 6000, visited = 0, n;
          while ((n = walker.nextNode()) && budget > 0 && visited++ < 4000) {
            var r = document.createRange();
            r.selectNodeContents(n);
            var rects = r.getClientRects(), on = false;
            for (var i = 0; i < rects.length; i++) {
              var b = rects[i];
              if (b.width <= 0 || b.height <= 0) continue;
              var cx = (b.left + b.right) / 2, cy = (b.top + b.bottom) / 2;
              if (cx >= 0 && cx <= W && cy >= 0 && cy <= H) { on = true; break; }
            }
            if (on) { parts.push(n.nodeValue); budget -= n.nodeValue.length; }
          }
          var text = parts.join(' ').replace(/\s+/g, ' ').trim();
          if (!text) return '';
          var han = (text.match(
            /[぀-ヿ㐀-䶿一-鿿가-힯豈-﫿]/g) || []).length;
          var wide = han * 2 > text.length;
          var min = wide ? 12 : 30, max = wide ? 46 : 120;
          var chunks = text.split(/(?<=[.!?…。！？；;:：])\s*/);
          var picks = [];
          for (var j = 0; j < chunks.length; j++) {
            var c = chunks[j].trim();
            if (c.length >= min && c.length <= max && !/^[\s\W_]+${'$'}/.test(c)) picks.push(c);
          }
          if (!picks.length) {
            for (var k = 0; k + min <= text.length; k += max) {
              var w = text.substr(k, max).trim();
              if (w.length >= min) picks.push(w);
            }
          }
          if (!picks.length) picks = [text.substr(0, max).trim()];
          return picks[Math.floor(Math.random() * picks.length)];
        })()
    """.trimIndent()
}
