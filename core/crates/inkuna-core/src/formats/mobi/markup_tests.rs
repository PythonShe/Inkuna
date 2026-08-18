use super::normalize;

#[test]
fn normalizes_tag_soup_entities_and_safe_elements() {
    let input = concat!(
        r#"<p onclick="bad()">Before&nbsp;<b>bold<i>both</b>italic</i> &mdash; &amp; "#,
        r#"<custom data-x="bad">kept</custom><mbp:pagebreak/></p>"#,
        r#"<center style="color:red">middle</center><font face="serif">plain</font>"#,
    );

    let normalized = normalize(input, |_| None);

    assert_eq!(
        normalized.xhtml,
        concat!(
            "<p>Before\u{a0}<b>bold<i>both</i></b>italic — &amp; kept</p>",
            "<div class=\"mobi-center\">middle</div><span>plain</span>"
        )
    );
    assert_eq!(normalized.heading, None);
}

#[test]
fn filters_attributes_and_rewrites_recindex_images() {
    let input = concat!(
        r#"<a id="spot" href="ch00002.xhtml#fp20" style="bad">jump</a>"#,
        r#"<table><tr><td colspan="2" rowspan="3" width="9">cell</td></tr></table>"#,
        r#"<img recindex="7" src="old.gif" alt="old" onerror="bad"/>"#,
        r#"<img recindex="8"/><img src="kept.png" alt="cover" width="99"/>"#,
    );

    let normalized = normalize(input, |index| {
        (index == 7).then(|| "../images/img00007.png".to_string())
    });

    assert_eq!(
        normalized.xhtml,
        concat!(
            r#"<a href="ch00002.xhtml#fp20" id="spot">jump</a>"#,
            r#"<table><tr><td colspan="2" rowspan="3">cell</td></tr></table>"#,
            r#"<img src="../images/img00007.png" alt=""/>"#,
            r#"<img src="kept.png" alt="cover"/>"#,
        )
    );
}

#[test]
fn extracts_the_first_heading_and_closes_open_elements() {
    let normalized = normalize(
        "<div><h2>  First &amp; Only  </h2><h1>Second</h1><p>body<b>end",
        |_| None,
    );

    assert_eq!(normalized.heading.as_deref(), Some("First & Only"));
    assert_eq!(
        normalized.xhtml,
        "<div><h2>  First &amp; Only  </h2><h1>Second</h1><p>body<b>end</b></p></div>"
    );
}

#[test]
fn truncates_a_cjk_heading_on_character_boundaries() {
    let heading = "月".repeat(150);
    let normalized = normalize(&format!("<h1>{heading}</h1><p>松风入夜。</p>"), |_| {
        None
    });

    let extracted = normalized.heading.unwrap();
    assert_eq!(extracted.chars().count(), 120);
    assert!(extracted.chars().all(|ch| ch == '月'));
}

#[test]
fn removes_xml_illegal_controls_from_text_and_attributes() {
    let normalized = normalize(
        "<a id='a\u{1}b' href='chapter\u{7}.html'>x\u{0}y\t\nz</a>",
        |_| None,
    );

    assert_eq!(
        normalized.xhtml,
        "<a href=\"chapter.html\" id=\"ab\">xy\t\nz</a>"
    );
}

#[test]
fn drops_active_urls_and_remote_images() {
    let normalized = normalize(
        concat!(
            "<a href='javascript:alert(1)'>bad</a>",
            "<a href='https://example.com'>web</a>",
            "<img src='https://tracker.example/pixel.gif'/>",
            "<img src='../images/local.png' alt='local'/>",
        ),
        |_| None,
    );

    assert_eq!(
        normalized.xhtml,
        concat!(
            "<a>bad</a><a href=\"https://example.com\">web</a>",
            "<img src=\"../images/local.png\" alt=\"local\"/>"
        )
    );
}
