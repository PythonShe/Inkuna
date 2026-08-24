use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

use super::{parse_position, sanitize_css, ImageBudget, MAX_TOTAL_IMAGE_BYTES};
use crate::mobi::convert_to_epub;
use crate::test_support::{Kf8FileFixture, Kf8NcxFixture, MobiTestBuilder};
use inkuna_content as epub;

fn chapter(path: &Path, index: usize) -> String {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut value = String::new();
    archive
        .by_name(&format!("OEBPS/text/ch{index:05}.xhtml"))
        .unwrap()
        .read_to_string(&mut value)
        .unwrap();
    value
}

fn stylesheet(path: &Path) -> String {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut value = String::new();
    archive
        .by_name("OEBPS/style.css")
        .unwrap()
        .read_to_string(&mut value)
        .unwrap();
    value
}

#[test]
fn rewrites_kindle_positions_embeds_and_css_and_uses_ncx_nesting() {
    let dir = tempfile::tempdir().unwrap();
    let azw3 = dir.path().join("links.azw3");
    let epub_path = dir.path().join("links.epub");
    let first = concat!(
        "<html><head><link rel='stylesheet' href='kindle:flow:0001'/></head><body>",
        "<h1>Ignored heading</h1><section aid='old'><p>A</p></section>",
        "<a href='kindle:pos:fid:0000:off:0000000000'>same</a>",
        "<a href='kindle:pos:fid:0001:off:0000000000'>cross</a>",
        "<img src='kindle:embed:0001?mime=image/png' alt='art'/>",
        "</body></html>",
    );
    let second = "<html><body><article><h2>Second heading</h2><p>X</p></article></body></html>";
    let first_insert = first.find("</p>").unwrap();
    let second_insert = second.find("</p>").unwrap();
    let png = b"\x89PNG\r\n\x1a\nasset";
    MobiTestBuilder::new(8)
        .fullname("月光集".as_bytes())
        .locale(0x0804)
        .image(png)
        .kf8_files(vec![
            Kf8FileFixture::new(first.as_bytes()).fragment(first_insert, "一".as_bytes()),
            Kf8FileFixture::new(second.as_bytes()).fragment(second_insert, "二".as_bytes()),
        ])
        .css_flow(
            b"@import url('https://tracker.example/a.css'); p { color: black; background: url(//tracker.example/x); }",
        )
        .ncx(vec![
            Kf8NcxFixture::new("第一卷", 0, 1),
            Kf8NcxFixture::new("第二章", 1, 2),
        ])
        .write(&azw3);

    convert_to_epub(&azw3, &epub_path, "Fallback").unwrap();

    let package = epub::read_package(&epub_path).unwrap();
    assert_eq!(package.metadata.title.as_deref(), Some("月光集"));
    assert_eq!(package.spine.len(), 2);
    assert_eq!(
        package
            .toc
            .iter()
            .map(|entry| (entry.title.as_str(), entry.depth))
            .collect::<Vec<_>>(),
        [("第一卷", 0), ("第一卷", 1), ("第二章", 1)]
    );
    let first_chapter = chapter(&epub_path, 1);
    let second_chapter = chapter(&epub_path, 2);
    assert!(first_chapter.contains(r##"href="#kp0000""##));
    assert!(first_chapter.contains(r#"id="kp0000""#));
    assert!(first_chapter.contains(r#"href="ch00002.xhtml#kp0001""#));
    assert!(second_chapter.contains(r#"id="kp0001""#));
    assert!(first_chapter.contains(r#"src="../images/kf8img00001.png""#));
    assert!(!first_chapter.contains("kindle:"));
    assert!(!first_chapter.contains(" aid="));
    let css = stylesheet(&epub_path);
    assert!(css.contains("p { color: black;"));
    assert!(!css.contains("@import"));
    assert!(!css.contains("tracker.example"));
}

#[test]
fn missing_ncx_falls_back_to_headings_and_cjk_default_titles() {
    let dir = tempfile::tempdir().unwrap();
    let azw3 = dir.path().join("fallback.azw3");
    let epub_path = dir.path().join("fallback.epub");
    MobiTestBuilder::new(8)
        .locale(0x0804)
        .kf8_files(vec![
            Kf8FileFixture::new("<body><h1>序章</h1><p>松风。</p></body>".as_bytes()),
            Kf8FileFixture::new("<body><p>月影。</p></body>".as_bytes()),
        ])
        .write(&azw3);

    convert_to_epub(&azw3, &epub_path, "無題").unwrap();

    let package = epub::read_package(&epub_path).unwrap();
    assert_eq!(
        package
            .toc
            .iter()
            .map(|entry| entry.title.as_str())
            .collect::<Vec<_>>(),
        ["序章", "第2章"]
    );
    let text = crate::test_support::extract_spine_text(&epub_path, &package.spine_hrefs());
    assert!(text[0].as_deref().unwrap().contains("松风。"));
    assert!(text[1].as_deref().unwrap().contains("月影。"));
}

#[test]
fn css_sanitizer_removes_imports_and_remote_urls_but_keeps_local_rules() {
    let css = sanitize_css(
        "@import \"evil.css\"; .a{background:url(https://x/a.png)} .b{background:url('../ok.png')}",
    );
    assert!(!css.contains("@import"));
    assert!(!css.contains("https://"));
    assert!(css.contains("../ok.png"));
}

#[test]
fn css_sanitizer_catches_identifier_escaped_imports_and_urls() {
    let escaped_import = sanitize_css("@\\69 mport url(\"https://x\")");
    assert!(!escaped_import.contains("https://"));
    let escaped_url = sanitize_css(".a{background:\\75 rl(https://x)}");
    assert!(!escaped_url.contains("https://"));
}

/// Sanitizes `threat` repeated to fill `bytes`, returning the output and the
/// fastest of two runs. Scheduling noise only ever *adds* time, so the minimum
/// is the closest reading to the work actually done.
fn time_sanitize(threat: &str, bytes: usize) -> (String, Duration) {
    let input = threat.repeat(bytes / threat.len() + 1);
    let mut run = || {
        let started = Instant::now();
        let output = sanitize_css(&input);
        (output, started.elapsed())
    };
    let (_, first) = run();
    let (output, second) = run();
    (output, first.min(second))
}

/// `sanitize_css` iterates its strip to a fixed point, so a careless change
/// there turns a forward scan into a quadratic one. What pins that down is the
/// *shape* of the cost curve, not a stopwatch: an absolute wall-clock ceiling
/// is a property of the machine, and one tuned on a dev box fails on a shared
/// CI runner several times slower. Quadrupling the input instead cancels
/// machine speed out — linear work grows about 4x, quadratic work about 16x,
/// and anything in between is a regression worth failing on.
#[test]
fn css_sanitizer_handles_large_repeated_threats_in_linear_time() {
    const SMALL: usize = 1024 * 1024;
    const LARGE: usize = 4 * SMALL;
    // Sits between the 4x linear growth expected and the 16x a quadratic scan
    // would show, leaning toward the linear end so runner jitter cannot trip
    // it — a genuinely quadratic pass over 4 MiB overruns any bound by orders
    // of magnitude, so nothing is lost by being generous here.
    const TOLERANCE: u32 = 10;
    // A sub-millisecond baseline is mostly timer noise; dividing by it would
    // turn jitter into a failure, so the small run never sets a tighter budget
    // than this floor does.
    const NOISE_FLOOR: Duration = Duration::from_millis(20);

    for threat in ["@import;", "url(https://x)"] {
        let (_, small) = time_sanitize(threat, SMALL);
        let (output, large) = time_sanitize(threat, LARGE);

        let expected = match threat {
            "@import;" => String::new(),
            _ => "none".repeat(LARGE / threat.len() + 1),
        };
        assert_eq!(output, expected, "sanitizing {threat:?} at {LARGE} bytes");

        assert!(
            large < small.max(NOISE_FLOOR) * TOLERANCE,
            "sanitizing {threat:?} grew super-linearly: \
             {small:?} at {SMALL} bytes, {large:?} at {LARGE} bytes"
        );
    }
}

#[test]
fn image_budget_reserve_is_saturating_and_caps_the_total() {
    // Unit test of ImageBudget semantics only: reserve accumulates toward
    // MAX_TOTAL_IMAGE_BYTES and rejects what would exceed it. Whether the
    // cover block actually routes through this budget is not exercised
    // here — a fixture straddling the 128 MiB cap is impractical.
    let mut budget = ImageBudget { used: 0 };
    assert!(budget.reserve(MAX_TOTAL_IMAGE_BYTES - 1));
    assert!(!budget.reserve(2));
    assert!(budget.reserve(1));
    assert!(!budget.reserve(1));
}

#[test]
fn position_parser_accepts_the_full_ten_digit_base32_offset_space() {
    assert_eq!(
        parse_position("kindle:pos:fid:0001:off:VVVVVVVVVV"),
        Some(1)
    );
}

/// `strip_css_threats` is a single forward pass, so deleting one `@import`
/// can splice its neighbours into a fresh one: `@im` + `port "https://evil";`
/// re-forms `@import "https://evil";`. Nothing inside that pass catches the
/// re-formed rule — what closes the hole is `sanitize_css` iterating the
/// strip to a fixed point, so each pass consumes what the previous one
/// spliced together. This test pins the shallowest case; the deeper chains
/// live in `css_sanitizer_kills_multi_level_splice_chains`.
#[test]
fn css_sanitizer_kills_an_import_reformed_by_its_own_splice() {
    let spliced = sanitize_css("@im@import;port \"https://evil\";");
    assert!(
        !spliced.to_ascii_lowercase().contains("@import"),
        "splice re-formed a live @import: {spliced:?}"
    );
    assert!(
        !spliced.contains("evil"),
        "remote target survived: {spliced:?}"
    );

    // Mixed case, because the splice is found case-insensitively too.
    let mixed = sanitize_css("@im@IMPORT;port url(\"https://evil\");");
    assert!(
        !mixed.to_ascii_lowercase().contains("@import"),
        "mixed-case splice re-formed a live @import: {mixed:?}"
    );
    assert!(!mixed.contains("evil"), "remote target survived: {mixed:?}");
}

/// Builds an `@import` splice chain of `levels` levels: every `@im`/`port`
/// pair only meets to form a live `@import` once the pass before it deleted
/// the rule wedged between them, so the chain needs `levels + 2` strip passes
/// to settle. One level is `@im@import;port "…";`, two levels
/// `@im@im@import;port;port "…";`, and so on.
fn spliced_import_chain(levels: usize) -> String {
    let mut css = "@im".repeat(levels);
    css.push_str("@import;");
    css.push_str(&"port;".repeat(levels - 1));
    css.push_str("port \"https://evil\";");
    css
}

/// The same construction aimed at `url(`: the deletions splice `ur` onto
/// `l(https://evil)`.
fn spliced_url_chain(levels: usize) -> String {
    let mut css = String::from("ur");
    css.push_str(&"@im".repeat(levels - 1));
    css.push_str("@import;");
    css.push_str(&"port;".repeat(levels - 1));
    css.push_str("l(https://evil)");
    css
}

/// Resolves CSS identifier escapes the way a CSS parser does — once — so a
/// test can ask what the *parser* will see in what the sanitizer emitted.
fn unescaped_once(css: &str) -> String {
    let mut out = String::new();
    let mut chars = css.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let mut hex = String::new();
        while hex.len() < 6 && chars.peek().is_some_and(char::is_ascii_hexdigit) {
            hex.push(chars.next().unwrap());
        }
        if hex.is_empty() {
            if let Some(escaped) = chars.next() {
                out.push(escaped);
            }
            continue;
        }
        out.push(char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).unwrap_or('\u{fffd}'));
        if chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
    }
    out
}

fn assert_no_remote_reference(label: &str, css: &str) {
    let lowered = css.to_ascii_lowercase();
    assert!(
        !lowered.contains("@import"),
        "{label} re-formed a live @import: {css:?}"
    );
    assert!(!lowered.contains("url("), "{label} left a url(: {css:?}");
    assert!(!css.contains("evil"), "{label} kept a remote target: {css:?}");
}

/// Two passes only defeat one level of self-splicing: the two-level chain
/// `@im@im@import;port;port "https://evil";` used to come back out as a live
/// `@import "https://evil";`, i.e. an outbound fetch chosen by the ebook.
/// The fixed-point loop settles chains of any depth up to the cap.
#[test]
fn css_sanitizer_kills_multi_level_splice_chains() {
    for levels in 1..=6 {
        let imports = spliced_import_chain(levels);
        assert_no_remote_reference(
            &format!("{levels}-level import chain"),
            &sanitize_css(&imports),
        );

        let urls = spliced_url_chain(levels);
        assert_no_remote_reference(&format!("{levels}-level url chain"), &sanitize_css(&urls));
    }
}

/// Past the shared pass budget the sanitizer fails closed: it drops the whole
/// stylesheet rather than handing `append_css` a partially stripped one. The
/// benign sibling rule makes the difference observable — inside the budget it
/// survives, past it the entire sheet goes.
#[test]
fn css_sanitizer_drops_stylesheets_that_outrun_the_pass_cap() {
    let keep = ".keep{color:red}";
    assert_eq!(
        sanitize_css(&format!("{keep}{}", spliced_import_chain(12))),
        keep
    );
    assert_eq!(
        sanitize_css(&format!("{keep}{}", spliced_import_chain(13))),
        ""
    );
    assert_eq!(sanitize_css(&format!("{keep}{}", spliced_url_chain(64))), "");
}

/// A CSS parser resolves identifier escapes exactly once, so a *doubly*
/// escaped `\5c 75 rl(` reaches it as the ident `\75` beside an `rl(`
/// function — not as `url(`, and not as a fetch. It is left alone on purpose;
/// resolving escapes twice would be a threat model the parser does not share.
#[test]
fn css_sanitizer_leaves_doubly_escaped_text_alone_because_css_resolves_once() {
    let css = ".a{background:\\5c 75 rl(https://x)}";
    assert_eq!(sanitize_css(css), css);
}

/// Stripping the escape-resolved form can splice a *fresh* escape out of its
/// own deletion: `\5c 7` + `@import;` + `5 rl(...)` resolves to
/// `\7@import;5 rl(...)`, and deleting that rule joins the halves into
/// `\75 rl(...)` — which a parser *does* resolve into a live `url(`. One
/// strip-then-unescape round used to emit exactly that; the alternation keeps
/// going until resolving the escapes of what it is about to emit exposes
/// nothing left to strip.
#[test]
fn css_sanitizer_kills_escapes_spliced_by_the_strip_itself() {
    let url = sanitize_css("\\5c 7@\\69 mport;5 rl(https://evil)");
    assert_no_remote_reference("escape spliced into url(", &url);
    assert!(
        !unescaped_once(&url).to_ascii_lowercase().contains("url("),
        "escape resolves to a live url(: {url:?}"
    );

    let import = sanitize_css("@\\5c 6@\\69 mport;9 mport \"https://evil\";");
    assert_no_remote_reference("escape spliced into @import", &import);
    assert!(
        !unescaped_once(&import)
            .to_ascii_lowercase()
            .contains("@import"),
        "escape resolves to a live @import: {import:?}"
    );

    // Two levels deep: the splice that forms `\75 rl(` is itself only formed
    // by an earlier splice, so it takes another whole round to settle.
    let deeper = sanitize_css("\\5c 7@\\69 m@\\69 mport;port;5 rl(https://evil)");
    assert_no_remote_reference("two-level escape splice", &deeper);
    assert!(
        !unescaped_once(&deeper).to_ascii_lowercase().contains("url("),
        "escape resolves to a live url(: {deeper:?}"
    );
}
